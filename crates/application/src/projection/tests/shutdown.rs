//! Last-resort shutdown must retain uncertain storage and settle admitted callers.
use super::support::*;
use crate::process::OutputAcceptance;
use crate::projection::{ProjectionError, Residency};
use pty_runtime_domain::terminal::TerminalSize;
use std::sync::atomic::Ordering;

fn discard_accepted_job(h: &Harness) {
    // Simulate a failed executor that has stopped and dropped its accepted job.
    // No worker is concurrent with finish_after_shutdown. An absent completion
    // must not be mistaken for confirmed deletion or unpublished storage.
    assert_eq!(h.jobs.len(), 1);
    drop(h.jobs.take().unwrap());
    assert_eq!(h.jobs.len(), 0);
}

fn closed_with_uncertain_source(h: &Harness) {
    assert_eq!(h.owner.status().residency, Residency::Closed);
    assert_eq!(h.owner.close_outcome(), Some(Err(ProjectionError::Worker)));
    assert_eq!(h.budgets.unreclaimed_sources(), 1);
    assert!(h.budgets.unreclaimed_reserved_bytes() > 0);
    assert_eq!(h.probe.alive.load(Ordering::Acquire), 0);
    assert!(h.owner.wiring.services_released());
    assert!(h.owner.wiring.handle_released());
    assert!(h.owner.wiring.process_released());
    let resources = h.budgets.resources();
    assert_eq!(resources.native_reservations.used, 0);
    assert_eq!(resources.checkpoint_buffers.used, 0);
    assert_eq!(resources.staging_bytes.used, 0);
    assert_eq!(resources.staging_slots.used, 0);
}

#[test]
fn shutdown_with_lost_commit_completion_keeps_uncertain_disk_reservation() {
    let h = Harness::standard();
    h.clock.0.store(60, Ordering::Release);
    h.step();
    discard_accepted_job(&h);
    let mut close = h.owner.close().unwrap();
    h.owner.finish_after_shutdown();
    assert_eq!(result(&mut close), Err(ProjectionError::Worker));
    closed_with_uncertain_source(&h);
    assert_eq!(h.store.commits.load(Ordering::Acquire), 0);
    assert_eq!(h.store.deletes.load(Ordering::Acquire), 0);
    h.owner.finish_after_shutdown();
    assert_eq!(h.budgets.unreclaimed_sources(), 1);
}

#[test]
fn shutdown_with_lost_delete_completion_does_not_release_sole_source_charge() {
    let h = Harness::standard();
    h.park();
    let mut close = h.owner.close().unwrap();
    h.step();
    discard_accepted_job(&h);
    h.owner.finish_after_shutdown();
    assert_eq!(result(&mut close), Err(ProjectionError::Worker));
    closed_with_uncertain_source(&h);
    assert_eq!(h.store.entries.lock().unwrap().len(), 1);
    assert_eq!(h.store.deletes.load(Ordering::Acquire), 0);
}

#[test]
fn shutdown_with_lost_restore_completion_releases_live_memory_but_keeps_source() {
    let h = Harness::standard();
    h.park();
    assert_eq!(h.owner.stage_output(b"queued"), OutputAcceptance::Accepted);
    h.step();
    discard_accepted_job(&h);
    h.owner.finish_after_shutdown();
    closed_with_uncertain_source(&h);
    assert_eq!(h.store.entries.lock().unwrap().len(), 1);
    assert_eq!(h.store.deletes.load(Ordering::Acquire), 0);
}

#[test]
fn shutdown_with_lost_transfer_read_settles_caller_and_releases_observer() {
    let h = Harness::standard();
    h.park();
    let mut transfer = h.owner.begin_transfer().unwrap();
    h.step();
    discard_accepted_job(&h);
    h.owner.finish_after_shutdown();
    assert!(matches!(
        result(&mut transfer),
        Err(ProjectionError::Worker)
    ));
    drop(transfer);
    closed_with_uncertain_source(&h);
    assert_eq!(h.budgets.resources().transfer_observers.used, 0);
    assert_eq!(h.budgets.resources().requests.used, 0);
    assert_eq!(h.store.entries.lock().unwrap().len(), 1);
}

#[test]
fn shutdown_without_scheduler_rejects_queued_views_and_controls_and_releases_staging() {
    let h = Harness::standard();
    assert_eq!(
        h.owner.stage_output(b"unprocessed"),
        OutputAcceptance::Accepted
    );
    let mut view = h.owner.view().unwrap();
    let mut resize = h.owner.resize(TerminalSize::new(4, 2).unwrap()).unwrap();
    let mut checkpoint = h.owner.checkpoint().unwrap();
    h.owner.finish_after_shutdown();
    assert!(matches!(result(&mut view), Err(ProjectionError::Worker)));
    assert!(matches!(result(&mut resize), Err(ProjectionError::Worker)));
    assert!(matches!(
        result(&mut checkpoint),
        Err(ProjectionError::Worker)
    ));
    drop((view, resize, checkpoint));
    assert_eq!(h.owner.status().residency, Residency::Closed);
    assert_eq!(h.owner.status().processed.offset, 0);
    assert_eq!(h.budgets.unreclaimed_sources(), 0);
    assert_eq!(h.budgets.resources().staging_bytes.used, 0);
    assert_eq!(h.budgets.resources().requests.used, 0);
    assert_eq!(h.store.commits.load(Ordering::Acquire), 0);
    assert_eq!(h.probe.alive.load(Ordering::Acquire), 0);
}
