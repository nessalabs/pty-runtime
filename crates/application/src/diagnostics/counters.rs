use std::sync::atomic::{AtomicU64, Ordering};
/// Cumulative bounded operation counters. Values saturate instead of wrapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum CounterKind {
    /// Bytes returned by successful host PTY reads, before admission/backpressure.
    BytesRead,
    /// Bytes successfully written to the host PTY, including authoritative replies.
    BytesWritten,
    /// Input payload bytes admitted by the process adapter, including replies.
    InputAdmittedBytes,
    /// Bytes accepted into raw replay (including immediately evicted bytes).
    PublishedBytes,
    /// Gap events delivered to observers; multiple observers count separately.
    ObserverGaps,
    /// Lost-byte ranges delivered to observers; multiple observers count separately.
    ObserverGapBytes,
    /// Global public input or adapter queue byte/chunk/slot admission rejections.
    InputSaturation,
    /// Lossless output admission attempts that required backpressure.
    OutputBackpressure,
    /// Failed process operations or supervision reports; not unique sessions.
    FailedOperations,
    /// TERM grace expiration escalations to a KILL request, once per process owner.
    CancellationEscalations,
    /// Process owners whose helper cleanup completed without supervision loss.
    CleanupCompleted,
    /// Process owners that completed teardown with supervision loss.
    CleanupFailed,
    /// Observed helper acknowledgements of successful direct-workload SIGKILL
    /// escalation after TERM. Missing helper transport yields no acknowledgement;
    /// this does not claim an after-SIGKILL acknowledgement from group members.
    AcknowledgedWorkloadEscalations,
}
impl CounterKind {
    /// All aggregate counter names in snapshot order.
    pub const ALL: [Self; 13] = [
        Self::BytesRead,
        Self::BytesWritten,
        Self::InputAdmittedBytes,
        Self::PublishedBytes,
        Self::ObserverGaps,
        Self::ObserverGapBytes,
        Self::InputSaturation,
        Self::OutputBackpressure,
        Self::FailedOperations,
        Self::CancellationEscalations,
        Self::CleanupCompleted,
        Self::CleanupFailed,
        Self::AcknowledgedWorkloadEscalations,
    ];
}
/// Fixed aggregate counters plus current activity/retention gauges.
#[derive(Debug, Clone)]
pub struct AggregateSnapshot {
    /// Current admitted session contexts whose process/drain completion is unfinished.
    /// Includes in-flight spawn; failed registration is removed when its context drops.
    pub active_sessions: u64,
    /// Logical bytes currently retained in raw replay across surviving contexts.
    pub retained_replay_bytes: u64,
    /// Cumulative operation counters indexed by CounterKind.
    pub counters: [u64; 13],
}
impl AggregateSnapshot {
    /// Read one cumulative operation counter.
    pub fn count(&self, kind: CounterKind) -> u64 {
        self.counters[kind as usize]
    }
}
pub(super) struct Counters {
    values: [AtomicU64; 13],
    active: AtomicU64,
    retained: AtomicU64,
}
fn add(counter: &AtomicU64, count: u64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
        Some(value.saturating_add(count))
    });
}
fn subtract(counter: &AtomicU64, count: u64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
        Some(value.saturating_sub(count))
    });
}
impl Counters {
    pub fn new() -> Self {
        Self {
            values: std::array::from_fn(|_| AtomicU64::new(0)),
            active: AtomicU64::new(0),
            retained: AtomicU64::new(0),
        }
    }
    pub fn add(&self, kind: CounterKind, count: u64) {
        add(&self.values[kind as usize], count);
    }
    pub fn session(&self, active: bool) {
        if active {
            add(&self.active, 1);
        } else {
            subtract(&self.active, 1);
        }
    }
    pub fn replay(&self, before: usize, after: usize) {
        if after >= before {
            add(&self.retained, (after - before) as u64);
        } else {
            subtract(&self.retained, (before - after) as u64);
        }
    }
    pub fn reset_quiescent(&self) {
        for counter in &self.values {
            counter.store(0, Ordering::Relaxed);
        }
        // Current gauges survive measurement-interval resets.
    }
    pub fn snapshot(&self) -> AggregateSnapshot {
        AggregateSnapshot {
            active_sessions: self.active.load(Ordering::Relaxed),
            retained_replay_bytes: self.retained.load(Ordering::Relaxed),
            counters: std::array::from_fn(|index| self.values[index].load(Ordering::Relaxed)),
        }
    }
}
