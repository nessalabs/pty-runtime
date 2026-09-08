use super::RuntimeDiagnostics;
use std::sync::Arc;

/// Opt-in observation of one reader's already allocated, fixed-capacity scratch.
/// The adapter must create this guard after allocation using `Vec<u8>::capacity()`,
/// and retain it until that buffer is released. Drop subtracts the same observed
/// bytes on normal, error and unwinding exits; resetting diagnostics does not
/// reset these live gauges. This guard owns no scratch or operating-system state.
/// Concurrent snapshots are approximate; inspect after quiescence for exact totals.
pub struct ReaderAllocation {
    diagnostics: Arc<RuntimeDiagnostics>,
    bytes: usize,
}
impl RuntimeDiagnostics {
    /// Track an actual reader allocation, not configured size or session count.
    /// Call only after allocation succeeds and keep the returned guard until the
    /// fixed-capacity buffer is freed. Does not allocate per sample, alter reader
    /// behavior, or fail admission; the returned guard retains this diagnostics object.
    pub fn reader_allocation(self: &Arc<Self>, allocated_bytes: usize) -> ReaderAllocation {
        self.counters.reader(allocated_bytes, true);
        ReaderAllocation {
            diagnostics: self.clone(),
            bytes: allocated_bytes,
        }
    }
}
impl Drop for ReaderAllocation {
    fn drop(&mut self) {
        self.diagnostics.counters.reader(self.bytes, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::{CounterKind, LatencyKind};
    use std::time::Duration;

    #[test]
    fn live_reader_allocations_survive_reset_and_release_independently() {
        let diagnostics = RuntimeDiagnostics::new();
        let first = Vec::<u8>::with_capacity(4093);
        let second = Vec::<u8>::with_capacity(8191);
        let first_size = first.capacity() as u64;
        let second_size = second.capacity() as u64;
        let first_guard = diagnostics.reader_allocation(first.capacity());
        let second_guard = diagnostics.reader_allocation(second.capacity());
        diagnostics.session_activity(true);
        diagnostics.replay_retention(0, 23);
        diagnostics.count(CounterKind::BytesRead, 99);
        diagnostics.record(LatencyKind::RawOutput, Duration::from_millis(1), true);
        diagnostics.reset_quiescent();
        let snapshot = diagnostics.aggregate();
        assert_eq!(snapshot.live_readers, 2);
        assert_eq!(
            snapshot.reader_scratch_allocated_bytes,
            first_size + second_size
        );
        assert_eq!(snapshot.active_sessions, 1);
        assert_eq!(snapshot.retained_replay_bytes, 23);
        assert_eq!(snapshot.count(CounterKind::BytesRead), 0);
        assert_eq!(diagnostics.snapshot(LatencyKind::RawOutput).samples(), 0);
        drop(first);
        drop(first_guard);
        assert_eq!(diagnostics.aggregate().live_readers, 1);
        assert_eq!(
            diagnostics.aggregate().reader_scratch_allocated_bytes,
            second_size
        );
        drop(second);
        drop(second_guard);
        assert_eq!(diagnostics.aggregate().live_readers, 0);
        assert_eq!(diagnostics.aggregate().reader_scratch_allocated_bytes, 0);
    }

    #[test]
    fn allocation_guard_releases_on_error_and_unwind() {
        let diagnostics = RuntimeDiagnostics::new();
        let error = || -> Result<(), ()> {
            let buffer = vec![0u8; 17];
            let guard = diagnostics.reader_allocation(buffer.capacity());
            let _owned = (buffer, guard);
            assert_eq!(diagnostics.aggregate().live_readers, 1);
            Err(())
        };
        assert_eq!(error(), Err(()));
        assert_eq!(diagnostics.aggregate().live_readers, 0);
        assert_eq!(diagnostics.aggregate().reader_scratch_allocated_bytes, 0);
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let buffer = vec![0u8; 31];
                let guard = diagnostics.reader_allocation(buffer.capacity());
                let _owned = (buffer, guard);
                assert_eq!(diagnostics.aggregate().reader_scratch_allocated_bytes, 31);
                panic!("synthetic reader scope unwind");
            }))
            .is_err()
        );
        assert_eq!(diagnostics.aggregate().live_readers, 0);
        assert_eq!(diagnostics.aggregate().reader_scratch_allocated_bytes, 0);
    }
}
