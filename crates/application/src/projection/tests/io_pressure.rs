//! Shared I/O pressure may delay work, but cannot lose the sole checkpoint source.
use super::{support::*, terminal::Trace};
use crate::{
    process::OutputAcceptance,
    projection::{ProjectionError, ProjectionLimits, Residency, budgets::Lease},
    scheduling::WorkSchedule,
};
use pty_runtime_domain::checkpoint::CheckpointRef;
use std::{sync::atomic::Ordering, time::Duration};

fn saved(h: &Harness) -> CheckpointRef {
    assert_eq!(h.owner.stage_output(b"before"), OutputAcceptance::Accepted);
    h.pump();
    h.park();
    h.store.entries.lock().unwrap().values().next().unwrap().0
}
fn source_unchanged(h: &Harness, reference: CheckpointRef) {
    let entries = h.store.entries.lock().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries.get(&reference.key).unwrap().0, reference);
    assert_eq!(h.store.deletes.load(Ordering::Acquire), 0);
    assert_eq!(h.budgets.resources().stored_slots.used, 1);
    assert_eq!(h.budgets.resources().stored_bytes.used, 1064);
    assert_eq!(h.budgets.unreclaimed_sources(), 0);
}
fn all_released(h: &Harness) {
    assert_budgets_released(h);
    assert!(h.store.entries.lock().unwrap().is_empty());
    assert_eq!(h.probe.alive.load(Ordering::Acquire), 0);
}
fn no_restore(h: &Harness) {
    assert_eq!(
        h.jobs.len(),
        0,
        "read must not be submitted without admission"
    );
    assert_eq!(h.probe.alive.load(Ordering::Acquire), 0);
    assert!(
        !h.probe
            .trace
            .lock()
            .unwrap()
            .iter()
            .any(|e| matches!(e, Trace::Restore(..)))
    );
}

#[test]
fn transfer_memory_pressure_releases_partial_admission_and_preserves_saved_source() {
    let h = Harness::new(
        options(),
        ProjectionLimits {
            checkpoint_bytes: 2088,
            ..ProjectionLimits::default()
        },
    );
    let reference = saved(&h);
    // Another live reservation leaves room for plaintext but not protected bytes.
    let held = Lease::shared(h.budgets.checkpoints.clone(), 1064).unwrap();
    let mut transfer = h.owner.begin_transfer().unwrap();
    assert_eq!(h.step(), WorkSchedule::After(Duration::from_millis(5)));
    assert!(matches!(
        result(&mut transfer),
        Err(ProjectionError::Capacity)
    ));
    drop(transfer);
    assert_eq!(h.owner.status().residency, Residency::Parked);
    assert_eq!(h.owner.status().failure, None);
    no_restore(&h);
    source_unchanged(&h, reference);
    let r = h.budgets.resources();
    assert_eq!(
        r.checkpoint_buffers.used, 1064,
        "partial plaintext lease leaked"
    );
    assert_eq!(r.native_reservations.used, 0);
    assert_eq!(r.requests.used, 0);
    assert_eq!(r.transfer_observers.used, 0);
    assert_eq!(r.staging_slots.used, 0);
    drop(held);
    let mut fresh = h.owner.checkpoint().unwrap();
    h.pump();
    let pin = result(&mut fresh).unwrap();
    assert_eq!(pin.checkpoint().bytes, b"before");
    assert_eq!(pin.checkpoint().descriptor.processed.offset, 6);
    assert_eq!(h.probe.alive.load(Ordering::Acquire), 0);
    drop((pin, fresh));
    assert_eq!(h.budgets.resources().checkpoint_buffers.used, 0);
    source_unchanged(&h, reference);
    h.close();
    all_released(&h);
}

#[test]
fn restore_memory_or_resident_pressure_keeps_ordered_output_until_capacity_returns() {
    for resident_pressure in [false, true] {
        let h = Harness::new(
            options(),
            ProjectionLimits {
                checkpoint_bytes: 2088,
                resident_bytes: 4096,
                ..ProjectionLimits::default()
            },
        );
        let reference = saved(&h);
        let held = if resident_pressure {
            Lease::shared(h.budgets.resident.clone(), 4096).unwrap()
        } else {
            Lease::shared(h.budgets.checkpoints.clone(), 1064).unwrap()
        };
        assert_eq!(h.owner.stage_output(b"after"), OutputAcceptance::Accepted);
        let mut view = h.owner.view().unwrap();
        assert_eq!(h.step(), WorkSchedule::After(Duration::from_millis(5)));
        assert!(poll(&mut view).is_pending());
        assert_eq!(h.owner.status().residency, Residency::Parked);
        assert_eq!(h.owner.status().failure, None);
        assert_eq!(h.owner.status().processed.offset, 6);
        assert_eq!(h.owner.status().published.offset, 11);
        no_restore(&h);
        source_unchanged(&h, reference);
        let r = h.budgets.resources();
        assert_eq!(r.staging_bytes.used, 5);
        assert_eq!(r.requests.used, 1);
        assert_eq!(
            r.checkpoint_buffers.used,
            if resident_pressure { 0 } else { 1064 }
        );
        assert_eq!(
            r.native_reservations.used,
            if resident_pressure { 4096 } else { 0 }
        );
        drop(held);
        h.pump();
        let observed = result(&mut view).unwrap();
        assert_eq!(observed.view().cells[0].text, "beforeafter");
        assert_eq!(h.owner.status().processed.offset, 11);
        assert_eq!(h.owner.status().failure, None);
        let trace = h.probe.trace.lock().unwrap();
        assert_eq!(
            trace
                .iter()
                .filter(|e| matches!(e, Trace::Restore(..)))
                .count(),
            1
        );
        assert_eq!(
            trace
                .iter()
                .filter(|e| matches!(e, Trace::Feed(bytes) if bytes == b"after"))
                .count(),
            1
        );
        drop(trace);
        drop((observed, view));
        assert_eq!(h.budgets.resources().checkpoint_buffers.used, 0);
        assert_eq!(h.budgets.resources().staging_bytes.used, 0);
        assert_eq!(h.store.deletes.load(Ordering::Acquire), 1);
        assert_eq!(h.budgets.resources().stored_bytes.used, 0);
        h.close();
        all_released(&h);
    }
}

#[test]
fn rejected_garbage_delete_retains_the_same_source_and_live_output_keeps_progressing() {
    let h = Harness::standard();
    assert_eq!(h.owner.stage_output(b"a"), OutputAcceptance::Accepted);
    h.pump();
    h.clock.0.store(60, Ordering::Release);
    h.step();
    assert_eq!(h.jobs.len(), 1);
    // Invalidate this pending publication while the original terminal stays live.
    assert_eq!(h.owner.stage_output(b"b"), OutputAcceptance::Accepted);
    h.step();
    assert_eq!(h.owner.status().processed.offset, 2);
    assert!(h.jobs.run_one());
    let reference = h.store.entries.lock().unwrap().values().next().unwrap().0;
    h.jobs.reject.store(true, Ordering::Release);
    assert_eq!(h.step(), WorkSchedule::After(Duration::from_millis(5)));
    assert_eq!(h.jobs.len(), 0);
    source_unchanged(&h, reference);
    assert_eq!(h.probe.alive.load(Ordering::Acquire), 1);
    assert_eq!(h.owner.status().residency, Residency::Resident);
    assert_eq!(h.owner.stage_output(b"c"), OutputAcceptance::Accepted);
    h.step();
    assert_eq!(h.owner.status().processed.offset, 3);
    assert_eq!(h.owner.status().failure, None);
    assert_eq!(h.step(), WorkSchedule::After(Duration::from_millis(5)));
    source_unchanged(&h, reference);
    assert_eq!(h.jobs.len(), 0);
    h.jobs.reject.store(false, Ordering::Release);
    h.pump();
    assert_eq!(h.store.deletes.load(Ordering::Acquire), 1);
    assert!(!h.store.entries.lock().unwrap().contains_key(&reference.key));
    assert_eq!(h.budgets.resources().stored_bytes.used, 0);
    assert_eq!(h.budgets.resources().stored_slots.used, 0);
    assert_eq!(h.budgets.unreclaimed_sources(), 0);
    let mut checkpoint = h.owner.checkpoint().unwrap();
    h.pump();
    let pin = result(&mut checkpoint).unwrap();
    assert_eq!(pin.checkpoint().bytes, b"abc");
    drop((pin, checkpoint));
    h.close();
    assert_eq!(h.store.deletes.load(Ordering::Acquire), 1);
    all_released(&h);
}
