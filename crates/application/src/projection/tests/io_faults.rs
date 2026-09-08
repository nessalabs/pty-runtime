//! Storage read boundaries must preserve the sole source and settle callers exactly.
use super::support::*;
use crate::{
    process::OutputAcceptance,
    projection::{ProjectionError, Residency},
};
use pty_runtime_domain::checkpoint::{CheckpointError, ProtectedCheckpoint};
use std::sync::atomic::Ordering;

fn transient_permits_released(h: &Harness) {
    let resources = h.budgets.resources();
    assert_eq!(resources.checkpoint_buffers.used, 0);
    assert_eq!(resources.native_reservations.used, 0);
    assert_eq!(resources.requests.used, 0);
    assert_eq!(resources.transfer_observers.used, 0);
    assert_eq!(resources.staging_slots.used, 0);
    assert_eq!(resources.stored_slots.used, 1);
    assert_eq!(resources.stored_bytes.used, 1064);
    assert_eq!(h.store.entries.lock().unwrap().len(), 1);
    assert_eq!(h.store.deletes.load(Ordering::Acquire), 0);
}

fn truncate_ciphertext(h: &Harness) {
    let mut entries = h.store.entries.lock().unwrap();
    let (_, stored) = entries.values_mut().next().unwrap();
    *stored = ProtectedCheckpoint::new(
        stored.key,
        stored.descriptor.clone(),
        vec![0; stored.ciphertext().len() - 1],
    );
}

#[test]
fn transfer_read_rejects_short_ciphertext_without_restoring_or_losing_source() {
    let h = Harness::standard();
    h.park();
    truncate_ciphertext(&h);
    let mut transfer = h.owner.begin_transfer().unwrap();
    h.pump();
    assert!(matches!(
        result(&mut transfer),
        Err(ProjectionError::Storage(CheckpointError::Unavailable))
    ));
    drop(transfer);
    assert_eq!(h.owner.status().residency, Residency::Parked);
    assert_eq!(h.owner.status().failure, None);
    transient_permits_released(&h);
    h.close();
}

#[test]
fn malformed_restore_read_fails_pending_view_but_retains_unprocessed_bytes() {
    let h = Harness::standard();
    assert_eq!(h.owner.stage_output(b"before"), OutputAcceptance::Accepted);
    h.pump();
    h.park();
    truncate_ciphertext(&h);
    assert_eq!(h.owner.stage_output(b"after"), OutputAcceptance::Accepted);
    let mut view = h.owner.view().unwrap();
    h.pump();
    let error = ProjectionError::Storage(CheckpointError::Unavailable);
    assert!(matches!(result(&mut view), Err(e) if e == error));
    drop(view);
    assert_eq!(h.owner.status().failure, Some(error));
    assert_eq!(h.owner.status().residency, Residency::Failed);
    assert_eq!(h.owner.status().processed.offset, 6);
    assert_eq!(h.owner.status().published.offset, 11);
    assert_eq!(h.budgets.resources().staging_bytes.used, 5);
    assert_eq!(h.budgets.resources().checkpoint_buffers.used, 0);
    assert_eq!(h.budgets.resources().native_reservations.used, 0);
    assert_eq!(h.budgets.resources().requests.used, 0);
    assert_eq!(h.store.entries.lock().unwrap().len(), 1);
    assert_eq!(h.store.deletes.load(Ordering::Acquire), 0);
    h.close();
    assert_eq!(h.budgets.resources().staging_bytes.used, 0);
}

#[test]
fn opened_checkpoint_contract_violation_is_invalid_configuration_and_releases_read_permits() {
    for wrong_descriptor in [true, false] {
        let h = Harness::standard();
        h.park();
        h.protector
            .wrong_open_descriptor
            .store(wrong_descriptor, Ordering::Release);
        if !wrong_descriptor {
            h.protector.opened_capacity.store(2048, Ordering::Release);
        }
        let mut transfer = h.owner.begin_transfer().unwrap();
        h.pump();
        assert!(matches!(
            result(&mut transfer),
            Err(ProjectionError::InvalidConfiguration)
        ));
        drop(transfer);
        assert_eq!(h.owner.status().residency, Residency::Parked);
        transient_permits_released(&h);
        h.close();
    }
}

#[test]
fn panicking_read_settles_transfer_as_worker_error_and_allows_a_fresh_read() {
    let h = Harness::standard();
    h.owner.stage_output(b"saved");
    h.pump();
    h.park();
    h.store.panic_read.store(true, Ordering::Release);
    let mut transfer = h.owner.begin_transfer().unwrap();
    h.pump();
    assert!(matches!(
        result(&mut transfer),
        Err(ProjectionError::Worker)
    ));
    drop(transfer);
    transient_permits_released(&h);
    assert_eq!(h.owner.status().residency, Residency::Parked);
    h.store.panic_read.store(false, Ordering::Release);
    let mut fresh = h.owner.checkpoint().unwrap();
    h.pump();
    let pin = result(&mut fresh).unwrap();
    assert_eq!(pin.checkpoint().bytes, b"saved");
    drop((pin, fresh));
    transient_permits_released(&h);
    h.close();
}

#[test]
fn rejected_transfer_read_releases_admission_and_can_be_retried() {
    let h = Harness::standard();
    h.park();
    h.jobs.reject.store(true, Ordering::Release);
    let mut transfer = h.owner.begin_transfer().unwrap();
    h.step();
    assert!(matches!(
        result(&mut transfer),
        Err(ProjectionError::Capacity)
    ));
    drop(transfer);
    assert_eq!(h.jobs.len(), 0);
    transient_permits_released(&h);
    assert_eq!(h.owner.status().residency, Residency::Parked);
    h.jobs.reject.store(false, Ordering::Release);
    let mut fresh = h.owner.checkpoint().unwrap();
    h.pump();
    drop(result(&mut fresh).unwrap());
    drop(fresh);
    transient_permits_released(&h);
    h.close();
}
