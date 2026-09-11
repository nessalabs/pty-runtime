use super::support::*;
use crate::{
    process::OutputAcceptance,
    projection::{ProjectionError, ProjectionLimits, Residency},
};
use std::sync::atomic::Ordering;
#[test]
fn failed_restore_retains_source_and_bounded_unprocessed_output_without_empty_model() {
    let h = Harness::standard();
    h.owner.stage_output(b"before");
    h.pump();
    h.park();
    h.probe.fail_restore.store(true, Ordering::Release);
    h.owner.stage_output(b"after");
    h.pump();
    assert_eq!(h.owner.status().residency, Residency::Failed);
    assert_eq!(h.owner.status().processed.offset, 6);
    assert_eq!(h.owner.status().published.offset, 11);
    assert_eq!(h.probe.alive.load(Ordering::Acquire), 0);
    assert_eq!(h.store.entries.lock().unwrap().len(), 1);
    h.close();
}
#[test]
fn close_during_pending_commit_deletes_late_result_and_releases_services() {
    let h = Harness::standard();
    h.owner.stage_output(b"state");
    h.pump();
    h.clock.0.store(60, Ordering::Release);
    h.step();
    let mut closed = h.owner.close().unwrap();
    h.step();
    assert!(poll(&mut closed).is_pending());
    assert_eq!(h.probe.alive.load(Ordering::Acquire), 0);
    h.jobs.run_one();
    h.pump();
    result(&mut closed).unwrap();
    assert_eq!(h.owner.status().residency, Residency::Closed);
    assert!(h.store.entries.lock().unwrap().is_empty());
    assert!(h.owner.wiring.services().is_err());
    assert_eq!(h.owner.stage_output(b"late"), OutputAcceptance::Closed);
}
#[test]
fn close_progresses_with_every_normal_request_slot_held() {
    let mut opts = options();
    opts.request_slots = 1;
    let h = Harness::new(opts, ProjectionLimits::default());
    let mut held = h.owner.view().unwrap();
    h.pump();
    assert!(matches!(h.owner.close(), Err(ProjectionError::Capacity)));
    h.pump();
    assert_eq!(h.owner.status().residency, Residency::Closed);
    assert!(result(&mut held).is_ok());
}
#[test]
fn permanent_delete_failure_closes_truthfully_and_stays_charged_in_bounded_ledger() {
    let limits = ProjectionLimits {
        stored_slots: 1,
        ..ProjectionLimits::default()
    };
    let h = Harness::new(options(), limits);
    h.park();
    h.store.fail_delete.store(true, Ordering::Release);
    let mut closed = h.owner.close().unwrap();
    h.pump();
    assert!(matches!(
        result(&mut closed),
        Err(ProjectionError::Storage(_))
    ));
    assert_eq!(h.owner.status().residency, Residency::Closed);
    assert!(h.owner.status().failure.is_some());
    assert_eq!(h.budgets.unreclaimed_sources(), 1);
    assert_eq!(h.budgets.unreclaimed_reserved_bytes(), 1064);
    assert_eq!(h.store.deletes.load(Ordering::Acquire), 3);
    assert!(h.owner.wiring.services().is_err());
    assert!(!h.budgets.stored_slots.acquire(1));
}
#[test]
fn failed_storage_has_finite_retries_and_retains_original_live_model() {
    let h = Harness::standard();
    h.store.fail_commit.store(true, Ordering::Release);
    h.clock.0.store(60, Ordering::Release);
    h.pump();
    for seconds in [65, 70, 75, 100] {
        h.clock.0.store(seconds, Ordering::Release);
        h.pump();
    }
    assert_eq!(h.store.commits.load(Ordering::Acquire), 3);
    assert_eq!(h.probe.alive.load(Ordering::Acquire), 1);
    assert_eq!(h.owner.status().residency, Residency::Resident);
    assert!(h.owner.status().parking_failure.is_some());
    h.close();
}
#[test]
fn rejected_blocking_admission_schedules_bounded_retry_without_losing_model() {
    let h = Harness::standard();
    h.jobs.reject.store(true, Ordering::Release);
    h.clock.0.store(60, Ordering::Release);
    assert!(matches!(
        h.step(),
        crate::scheduling::WorkSchedule::After(_)
    ));
    assert_eq!(h.probe.alive.load(Ordering::Acquire), 1);
    h.jobs.reject.store(false, Ordering::Release);
    h.clock.0.store(65, Ordering::Release);
    h.pump();
    assert_eq!(h.owner.status().residency, Residency::Parked);
    h.close();
}

#[test]
fn authentication_failure_retains_only_source_and_close_releases_pool_handles() {
    let h = Harness::standard();
    h.owner.stage_output(b"before");
    h.pump();
    h.park();
    h.protector.fail.store(true, Ordering::Release);
    h.owner.stage_output(b"after");
    h.pump();
    assert!(matches!(
        h.owner.status().failure,
        Some(ProjectionError::Storage(_))
    ));
    assert_eq!(h.store.entries.lock().unwrap().len(), 1);
    h.close();
    assert!(h.owner.wiring.handle().is_none());
    assert!(h.owner.wiring.services().is_err());
}

#[test]
fn uncertain_commits_stay_charged_and_report_failure_on_every_close() {
    for panic in [false, true] {
        let h = Harness::standard();
        h.store.panic_commit.store(panic, Ordering::Release);
        h.store.invalid_reference.store(!panic, Ordering::Release);
        h.clock.0.store(60, Ordering::Release);
        h.pump();
        assert_eq!(h.owner.status().residency, Residency::Resident);
        assert_eq!(h.probe.alive.load(Ordering::Acquire), 1);
        assert_eq!(h.budgets.unreclaimed_sources(), 1);
        assert_eq!(h.budgets.unreclaimed_reserved_bytes(), 1064);
        let mut closed = h.owner.close().unwrap();
        h.pump();
        assert!(result(&mut closed).is_err());
        assert!(result(&mut h.owner.close().unwrap()).is_err());
        assert_eq!(h.store.deletes.load(Ordering::Acquire), 0);
        assert_eq!(h.store.entries.lock().unwrap().len(), 1);
        assert!(h.owner.wiring.services().is_err());
    }
}

#[test]
fn scheduler_shutdown_fallback_finishes_late_commit_without_retaining_services() {
    let h = Harness::standard();
    h.clock.0.store(60, Ordering::Release);
    h.step();
    let mut closed = h.owner.close().unwrap();
    // Simulate joining accepted blocking work after the scheduler has stopped.
    assert!(h.jobs.run_one());
    h.owner.finish_after_shutdown();
    assert_eq!(result(&mut closed), Err(ProjectionError::Worker));
    assert_eq!(h.owner.status().residency, Residency::Closed);
    assert_eq!(h.budgets.unreclaimed_sources(), 1);
    assert_eq!(h.probe.alive.load(Ordering::Acquire), 0);
    assert!(h.owner.wiring.services().is_err());
    assert!(h.owner.wiring.handle().is_none());
    assert!(h.owner.wiring.process().is_none());
    h.owner.finish_after_shutdown();
    assert_eq!(h.budgets.unreclaimed_sources(), 1);
}

/// The delete retry bound is the configured one, not a constant.
///
/// `max_delete_attempts` was split out of `max_park_attempts` because the two
/// encode different things: how hard to try to *create* a parked source versus
/// how hard to try to *remove* one. Nothing pinned the new knob to its effect,
/// so a future change could quietly ignore it the way the split briefly did.
#[test]
fn configured_delete_attempts_bound_the_retries_actually_made() {
    for attempts in [1u32, 2, 5] {
        let mut projection = options();
        projection.max_delete_attempts = attempts;
        let limits = ProjectionLimits {
            stored_slots: 1,
            ..ProjectionLimits::default()
        };
        let h = Harness::new(projection, limits);
        h.park();
        h.store.fail_delete.store(true, Ordering::Release);
        let mut closed = h.owner.close().unwrap();
        h.pump();
        assert!(matches!(
            result(&mut closed),
            Err(ProjectionError::Storage(_))
        ));
        assert_eq!(
            h.store.deletes.load(Ordering::Acquire),
            attempts as usize,
            "exactly the configured number of attempts is made"
        );
        assert_eq!(h.budgets.unreclaimed_sources(), 1);
    }
}

/// A close grants a source that was already given up on one more round.
///
/// `SourceReaper::restore_attempts` is only reachable when the retries were
/// spent *before* the close: during a close, a source whose attempts run out is
/// surrendered to the ledger on the very next cleanup pass, so there is nothing
/// left to restore. Here a park is rolled back during normal serving, its stale
/// source exhausts its deletes while the projection is still live, and only then
/// does a close ask for cleanup again.
#[test]
fn closing_after_giving_up_mid_service_retries_the_abandoned_source() {
    let mut projection = options();
    projection.max_delete_attempts = 1;
    let h = Harness::new(projection, ProjectionLimits::default());
    h.owner.stage_output(b"a");
    h.pump();

    // Park, then stage output before the commit lands so release is refused and
    // the committed source becomes stale rather than authoritative.
    h.clock.0.store(60, Ordering::Release);
    h.step();
    h.owner.stage_output(b"b");
    h.step();
    h.store.fail_delete.store(true, Ordering::Release);
    h.jobs.run_one();
    h.step();

    // The stale source is deleted while still serving; that delete fails and,
    // with one attempt configured, is given up on outside any close.
    h.jobs.run_one();
    h.pump();
    assert_eq!(h.store.deletes.load(Ordering::Acquire), 1);
    assert_eq!(h.owner.status().residency, Residency::Resident);

    // Now close with deletion working again: the abandoned source is retried
    // rather than surrendered unreclaimed.
    h.store.fail_delete.store(false, Ordering::Release);
    let mut closed = h.owner.close().unwrap();
    h.pump();
    assert_eq!(
        h.store.deletes.load(Ordering::Acquire),
        2,
        "the source given up on before the close is attempted again"
    );
    assert!(result(&mut closed).is_ok());
    assert_eq!(h.budgets.unreclaimed_sources(), 0);
}
