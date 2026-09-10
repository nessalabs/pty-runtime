//! Additional owner-lifetime boundaries identified by the acceptance gap audit.
use super::support::*;
use crate::projection::{ProjectionLimits, Residency};

#[test]
fn cancelled_parked_transfer_keeps_inflight_memory_charged_until_close_drains_read() {
    let mut options = options();
    options.transfer_observers = 1;
    let h = Harness::new(options, ProjectionLimits::default());
    h.owner.stage_output(b"retained");
    h.pump();
    h.park();
    let transfer = h.owner.begin_transfer().unwrap();
    h.step(); // Admit the blocking read, but leave the executor at its barrier.
    assert_eq!(h.jobs.len(), 1);
    let admitted = h.budgets.resources();
    assert!(admitted.checkpoint_buffers.used > 0);
    assert_eq!(admitted.transfer_observers.used, 1);
    drop(transfer);
    let mut close = h.owner.close().unwrap();
    h.step();
    assert!(poll(&mut close).is_pending());
    assert_eq!(h.store.entries.lock().unwrap().len(), 1);
    assert_eq!(
        h.budgets.resources().checkpoint_buffers.used,
        admitted.checkpoint_buffers.used
    );
    // The accepted read can still produce plaintext after its consumer cancelled.
    // Closing must drain that ownership, then delete the immutable parked source.
    assert!(h.jobs.run_one());
    h.pump();
    result(&mut close).unwrap();
    assert_eq!(h.owner.status().residency, Residency::Closed);
    assert!(h.store.entries.lock().unwrap().is_empty());
    // A completed operation still owns its admitted waiter until the caller drops it.
    assert_eq!(h.budgets.resources().requests.used, 1);
    drop(close);
    let released = h.budgets.resources();
    assert_eq!(released.checkpoint_buffers.used, 0);
    assert_eq!(released.transfer_observers.used, 0);
    assert_eq!(released.stored_bytes.used, 0);
    assert_eq!(released.stored_slots.used, 0);
    assert_eq!(released.requests.used, 0);
    assert_eq!(released.staging_slots.used, 0);
    assert!(h.owner.wiring.services().is_err());
}
