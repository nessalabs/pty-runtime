use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
const BUCKETS: usize = 1025;
const WIDTH_US: u64 = 100;

/// Bounded success histogram and explicit failure count. Bucket upper bounds are
/// conservative; overflow percentiles return the observed exact maximum.
#[derive(Debug, Clone)]
pub struct LatencySnapshot {
    /// Successes distributed across fixed 100us buckets and one overflow bucket.
    pub buckets: [u64; BUCKETS],
    /// Failed or abandoned operations; never silently included as successful samples.
    pub failures: u64,
    /// Requested boundaries not observed by an injected adapter.
    pub unavailable: u64,
    /// Maximum successful elapsed time, rounded up to microseconds.
    pub maximum_us: u64,
}
impl LatencySnapshot {
    /// Success sample count; saturation is reported as u64::MAX.
    pub fn samples(&self) -> u64 {
        self.buckets
            .iter()
            .fold(0u64, |total, value| total.saturating_add(*value))
    }
    /// Conservative percentile in microseconds; None for no samples or an invalid percentile.
    pub fn percentile_upper_us(&self, percentile: u8) -> Option<u64> {
        let samples = self.samples();
        if samples == 0 || percentile == 0 || percentile > 100 {
            return None;
        }
        let rank = (u128::from(samples) * u128::from(percentile)).div_ceil(100);
        let mut total = 0u128;
        for (index, count) in self.buckets.iter().enumerate() {
            total += u128::from(*count);
            if total >= rank {
                return Some(if index == BUCKETS - 1 {
                    self.maximum_us
                } else {
                    ((index as u64 + 1) * WIDTH_US).min(self.maximum_us)
                });
            }
        }
        None
    }
}
pub(super) struct Histogram {
    buckets: [AtomicU64; BUCKETS],
    failures: AtomicU64,
    unavailable: AtomicU64,
    maximum_us: AtomicU64,
}
fn increment(counter: &AtomicU64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
        Some(value.saturating_add(1))
    });
}
impl Histogram {
    pub fn new() -> Self {
        Self {
            buckets: std::array::from_fn(|_| AtomicU64::new(0)),
            failures: AtomicU64::new(0),
            unavailable: AtomicU64::new(0),
            maximum_us: AtomicU64::new(0),
        }
    }
    pub fn reset_quiescent(&self) {
        for bucket in &self.buckets {
            bucket.store(0, Ordering::Relaxed);
        }
        self.failures.store(0, Ordering::Relaxed);
        self.unavailable.store(0, Ordering::Relaxed);
        self.maximum_us.store(0, Ordering::Relaxed);
    }
    pub fn unavailable(&self) {
        increment(&self.unavailable);
    }
    pub fn record(&self, elapsed: Duration, success: bool) {
        if !success {
            increment(&self.failures);
            return;
        }
        let micros = elapsed.as_nanos().div_ceil(1000).min(u128::from(u64::MAX)) as u64;
        self.maximum_us.fetch_max(micros, Ordering::Relaxed);
        let bucket = (micros.saturating_sub(1) / WIDTH_US).min((BUCKETS - 1) as u64) as usize;
        increment(&self.buckets[bucket]);
    }
    pub fn snapshot(&self) -> LatencySnapshot {
        LatencySnapshot {
            buckets: std::array::from_fn(|index| self.buckets[index].load(Ordering::Relaxed)),
            failures: self.failures.load(Ordering::Relaxed),
            unavailable: self.unavailable.load(Ordering::Relaxed),
            maximum_us: self.maximum_us.load(Ordering::Relaxed),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn boundary_rounding_overflow_and_failed_samples_remain_explicit() {
        let histogram = Histogram::new();
        for micros in [0, 100, 101, 20_000, 20_001, 102_400, 102_401, 1_000_000] {
            histogram.record(Duration::from_micros(micros), true);
        }
        histogram.record(Duration::from_secs(10), false);
        let snapshot = histogram.snapshot();
        assert_eq!(snapshot.samples(), 8);
        assert_eq!(snapshot.failures, 1);
        assert_eq!(snapshot.buckets[0], 2);
        assert_eq!(snapshot.buckets[199], 1);
        assert_eq!(snapshot.buckets[200], 1);
        assert_eq!(snapshot.buckets[1023], 1);
        assert_eq!(snapshot.buckets[1024], 2);
        assert_eq!(snapshot.percentile_upper_us(50), Some(20_000));
        assert_eq!(snapshot.percentile_upper_us(99), Some(1_000_000));
        assert_eq!(snapshot.percentile_upper_us(0), None);
        histogram.unavailable();
        histogram.reset_quiescent();
        histogram.record(Duration::from_micros(3), true);
        let measured = histogram.snapshot();
        assert_eq!(measured.samples(), 1);
        assert_eq!(measured.maximum_us, 3);
        assert_eq!(measured.failures, 0);
        assert_eq!(measured.unavailable, 0);
        assert_eq!(Histogram::new().snapshot().percentile_upper_us(99), None);
    }
}
