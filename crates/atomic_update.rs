//! Conditional atomic update for the 1.85 compiler and for current stable.
//!
//! Current stable renamed `Atomic::fetch_update` to `try_update` and denies the
//! old name when warnings are denied. `try_update` is not in 1.85, and
//! `cfg(version)` is still an unstable attribute, so neither name compiles on
//! both compilers. The operation those methods perform is a load and a weak
//! compare-exchange, which both compilers have.
//!
//! Application may depend only on the domain, and a compiler workaround is not
//! a domain rule, so this file is compiled into the application and
//! infrastructure crates rather than living in either layer's dependency list.
//! `Ok` is the value before the store. Callers that hand that value out — owner
//! identities and spawn generations — are wrong if it is the stored value
//! instead. `Err` is the value observed when the closure returns `None`, and
//! nothing is stored.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// See the module note for the `Ok` / `Err` contract.
pub(crate) fn try_update_u64(
    atomic: &AtomicU64,
    set: Ordering,
    fetch: Ordering,
    mut update: impl FnMut(u64) -> Option<u64>,
) -> Result<u64, u64> {
    let mut previous = atomic.load(fetch);
    while let Some(next) = update(previous) {
        match atomic.compare_exchange_weak(previous, next, set, fetch) {
            Ok(seen) => return Ok(seen),
            Err(seen) => previous = seen,
        }
    }
    Err(previous)
}

/// See the module note for the `Ok` / `Err` contract. Same loop as
/// [`try_update_u64`].
pub(crate) fn try_update_usize(
    atomic: &AtomicUsize,
    set: Ordering,
    fetch: Ordering,
    mut update: impl FnMut(usize) -> Option<usize>,
) -> Result<usize, usize> {
    let mut previous = atomic.load(fetch);
    while let Some(next) = update(previous) {
        match atomic.compare_exchange_weak(previous, next, set, fetch) {
            Ok(seen) => return Ok(seen),
            Err(seen) => previous = seen,
        }
    }
    Err(previous)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_returns_the_value_before_the_store() {
        let atomic = AtomicU64::new(1);
        let seen = try_update_u64(&atomic, Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .unwrap();
        assert_eq!(
            seen, 1,
            "callers hand out the previous value, not the stored one"
        );
        assert_eq!(atomic.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn refusal_stores_nothing() {
        let atomic = AtomicUsize::new(3);
        let seen = try_update_usize(&atomic, Ordering::AcqRel, Ordering::Acquire, |value| {
            (value < 3).then_some(value + 1)
        });
        assert_eq!(seen, Err(3));
        assert_eq!(atomic.load(Ordering::Acquire), 3);
        let saturated = AtomicU64::new(u64::MAX);
        let seen = try_update_u64(&saturated, Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        });
        assert_eq!(seen, Err(u64::MAX));
        assert_eq!(saturated.load(Ordering::Relaxed), u64::MAX);
    }

    #[test]
    fn lost_races_retry_and_each_previous_value_is_issued_once() {
        let atomic = AtomicU64::new(0);
        let mut issued = std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for _ in 0..4 {
                handles.push(scope.spawn(|| {
                    let mut seen = Vec::with_capacity(50);
                    for _ in 0..50 {
                        seen.push(
                            try_update_u64(&atomic, Ordering::AcqRel, Ordering::Acquire, |value| {
                                value.checked_add(1)
                            })
                            .unwrap(),
                        );
                    }
                    seen
                }));
            }
            let mut all = Vec::new();
            for handle in handles {
                all.extend(handle.join().unwrap());
            }
            all
        });
        issued.sort();
        assert_eq!(issued, (0..200).collect::<Vec<_>>());
        assert_eq!(atomic.load(Ordering::Acquire), 200);
    }
}
