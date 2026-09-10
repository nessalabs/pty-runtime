//! Closing may finish on an already scheduled worker before the caller wakes it.
use super::support::*;
use crate::{
    projection::{ProjectionError, Residency},
    scheduling::{ICapacitySignal, IScheduledWork, IWorkHandle, SchedulingError},
};
use pty_runtime_domain::checkpoint::CheckpointError;
use std::{
    sync::{Arc, Mutex, atomic::Ordering},
    time::Instant,
};

#[derive(Default)]
struct CompleteOnNotify {
    once: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}
impl ICapacitySignal for CompleteOnNotify {
    fn generation(&self) -> u64 {
        0
    }
    fn notify(&self) {
        let callback = self.once.lock().unwrap().take();
        if let Some(callback) = callback {
            callback();
        }
    }
    fn wait_after(&self, _: u64, _: Instant) {}
}
fn finish_between_publication_and_wake(h: &Harness) {
    let weak = Arc::downgrade(&h.owner);
    let jobs = h.jobs.clone();
    let signal = Arc::new(CompleteOnNotify::default());
    *signal.once.lock().unwrap() = Some(Box::new(move || {
        let owner = weak.upgrade().unwrap();
        assert_eq!(owner.status().residency, Residency::Closing);
        for _ in 0..32 {
            owner.run();
            jobs.run_one();
            if owner.status().residency == Residency::Closed {
                assert!(owner.wiring.handle_released());
                return;
            }
        }
        panic!("real cleanup worker did not finish within bounded steps");
    }));
    h.owner
        .wiring
        .inject_services(|services| services.capacity = signal);
}
fn released(h: &Harness, stored: usize) {
    let r = h.budgets.resources();
    for usage in [
        r.journal_bytes,
        r.journal_slots,
        r.transfer_observers,
        r.staging_bytes,
        r.staging_slots,
        r.native_reservations,
        r.checkpoint_buffers,
        r.views,
        r.requests,
    ] {
        assert_eq!(usage.used, 0);
    }
    assert_eq!(r.stored_slots.used, stored);
    assert_eq!(r.stored_bytes.used, stored * 1064);
    assert_eq!(h.probe.alive.load(Ordering::Acquire), 0);
    assert!(h.owner.wiring.services_released());
    assert!(h.owner.wiring.handle_released());
}

#[test]
fn completed_cleanup_before_close_wake_returns_success_and_releases_every_budget() {
    let h = Harness::standard();
    h.owner.stage_output(b"retained");
    h.pump();
    finish_between_publication_and_wake(&h);
    let admitted = h.owner.close();
    assert_eq!(h.owner.close_outcome(), Some(Ok(())));
    assert!(admitted.is_ok(), "completed close reported wake failure");
    let mut wait = admitted.unwrap();
    assert_eq!(result(&mut wait), Ok(()));
    drop(wait);
    released(&h, 0);
}

#[test]
fn completed_failed_cleanup_before_close_wake_preserves_storage_outcome() {
    let h = Harness::standard();
    h.park();
    h.store.fail_delete.store(true, Ordering::Release);
    finish_between_publication_and_wake(&h);
    let admitted = h.owner.close();
    let expected = Err(ProjectionError::Storage(CheckpointError::Unavailable));
    assert_eq!(h.owner.close_outcome(), Some(expected));
    assert!(
        admitted.is_ok(),
        "durable cleanup result hidden by wake failure"
    );
    let mut wait = admitted.unwrap();
    assert_eq!(result(&mut wait), expected);
    drop(wait);
    assert_eq!(h.store.deletes.load(Ordering::Acquire), 3);
    assert_eq!(h.budgets.unreclaimed_sources(), 1);
    released(&h, 1);
}

struct RejectWake;
impl IWorkHandle for RejectWake {
    fn wake(&self) -> Result<(), SchedulingError> {
        Err(SchedulingError::Closed)
    }
    fn close(&self) {}
}
#[test]
fn unfinished_close_with_missing_or_rejecting_scheduler_still_reports_worker() {
    for missing in [true, false] {
        let h = Harness::standard();
        h.owner.wiring.inject_handle(if missing {
            None
        } else {
            Some(Arc::new(RejectWake))
        });
        assert!(matches!(h.owner.close(), Err(ProjectionError::Worker)));
        assert_eq!(h.owner.status().residency, Residency::Closing);
        assert_eq!(h.owner.close_outcome(), None);
        h.pump();
        assert_eq!(h.owner.close_outcome(), Some(Ok(())));
        released(&h, 0);
    }
}
