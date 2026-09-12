//! Closing may finish on an already scheduled worker before the caller wakes it.
use super::support::*;
use crate::{
    projection::{ProjectionError, ProjectionLimits, Residency},
    scheduling::{ICapacitySignal, IScheduledWork},
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
/// A projection wired to a capacity signal that can run cleanup to completion.
///
/// The signal is installed at construction, because a collaborator is only
/// swappable before `create` sees it. What it *does* is decided later, by
/// [`finish_between_publication_and_wake`], once there is an owner to drive and
/// the caller has set up the state the race is meant to catch.
fn racing_harness() -> (Harness, Arc<CompleteOnNotify>) {
    let signal = Arc::new(CompleteOnNotify::default());
    let installed = signal.clone();
    let h = Harness::with_services(options(), ProjectionLimits::default(), move |services| {
        services.capacity = installed;
    });
    (h, signal)
}
fn finish_between_publication_and_wake(h: &Harness, signal: &Arc<CompleteOnNotify>) {
    let weak = Arc::downgrade(&h.owner);
    let jobs = h.jobs.clone();
    *signal.once.lock().unwrap() = Some(Box::new(move || {
        let owner = weak.upgrade().unwrap();
        assert_eq!(owner.status().residency, Residency::Closing);
        for _ in 0..32 {
            owner.run();
            jobs.run_one();
            if owner.status().residency == Residency::Closed {
                assert!(owner.wiring.handle().is_none());
                return;
            }
        }
        panic!("real cleanup worker did not finish within bounded steps");
    }));
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
    assert!(h.owner.wiring.services().is_err());
    assert!(h.owner.wiring.handle().is_none());
}

#[test]
fn completed_cleanup_before_close_wake_returns_success_and_releases_every_budget() {
    let (h, signal) = racing_harness();
    h.owner.stage_output(b"retained");
    h.pump();
    finish_between_publication_and_wake(&h, &signal);
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
    let (h, signal) = racing_harness();
    h.park();
    h.store.fail_delete.store(true, Ordering::Release);
    finish_between_publication_and_wake(&h, &signal);
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

#[test]
fn unfinished_close_with_missing_or_rejecting_scheduler_still_reports_worker() {
    for missing in [true, false] {
        let h = Harness::standard();
        if missing {
            // The state cleanup leaves behind: the registration is gone.
            h.owner.wiring.take_handle();
        } else {
            // Still registered, but the scheduler now refuses every wake.
            h.scheduler.reject_wake.store(true, Ordering::Release);
        }
        assert!(matches!(h.owner.close(), Err(ProjectionError::Worker)));
        assert_eq!(h.owner.status().residency, Residency::Closing);
        assert_eq!(h.owner.close_outcome(), None);
        h.pump();
        assert_eq!(h.owner.close_outcome(), Some(Ok(())));
        released(&h, 0);
    }
}
