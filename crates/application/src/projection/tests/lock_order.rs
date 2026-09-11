//! The documented lock order, checked by running rather than by reading.
//!
//! `state.rs` charts three tiers acquired downward and never upward. Every lock
//! a projection takes now goes through a chokepoint that registers its tier, and
//! an inversion panics. These tests drive real work through those chokepoints.
//!
//! A "no inversion" assertion is worthless on its own — if nothing ever nested,
//! nothing was checked. So each test also asserts which nestings actually
//! happened, and `the_detector_itself_catches_an_inversion` proves the detector
//! is capable of failing.
use super::support::*;
use crate::{
    process::OutputAcceptance,
    projection::tier::{self, Tier},
};
use std::sync::{Arc, Barrier};

/// The detector must be able to fail, or every test using it proves nothing.
#[test]
#[should_panic(expected = "lock order inverted")]
fn the_detector_itself_catches_an_inversion() {
    tier::reset();
    let _outer = tier::enter(Tier::Admission);
    // Taking the workspace while holding admission is the inversion the chart
    // forbids: the worker takes them the other way round.
    let _inner = tier::enter(Tier::Workspace);
}

/// A real leaf lock must keep its registration for as long as the mutex is held.
///
/// The two tests above call `tier::enter` directly, so they exercise the
/// detector but not the instrumentation. This one goes through the actual
/// `leaf` helper: `inject_services` runs its closure while the services mutex is
/// held, so anything the closure acquires is nested under a leaf and must panic.
///
/// An earlier version of `leaf` registered the tier in a local that dropped when
/// the helper returned, while the caller still held the mutex — nesting under a
/// leaf went undetected, and no test here would have noticed.
#[test]
#[should_panic(expected = "lock order inverted")]
fn a_real_leaf_lock_holds_its_registration_for_the_whole_hold() {
    let h = Harness::standard();
    tier::reset();
    h.owner.wiring.inject_services(|_| {
        let _nested = tier::enter(Tier::Leaf);
    });
}

/// Two leaves must not nest either — they are leaves precisely because nothing
/// is acquired while one is held.
#[test]
#[should_panic(expected = "lock order inverted")]
fn the_detector_catches_one_leaf_taken_under_another() {
    tier::reset();
    let _outer = tier::enter(Tier::Leaf);
    let _inner = tier::enter(Tier::Leaf);
}

/// A worker run is the path that takes the outermost lock, so it is where the
/// documented workspace -> admission nesting has to show up.
#[test]
fn a_worker_run_nests_admission_under_the_workspace_and_never_inverts() {
    let h = Harness::standard();
    tier::reset();
    assert_eq!(h.owner.stage_output(b"hello"), OutputAcceptance::Accepted);
    h.pump();

    let seen = tier::observed_nestings();
    assert!(
        seen.contains(&(Tier::Workspace, Tier::Admission)),
        "the worker must have taken admission under the workspace; saw {seen:?}",
    );
    assert!(
        tier::holds_none(),
        "every lock taken during the run was released",
    );
}

/// The chart says a runtime-wide leaf is reached under the per-session
/// admission lock. That nesting is real and load-bearing, so it is asserted
/// rather than assumed.
///
/// It happens when an already-reserved `StagingLease` is dropped *inside* the
/// admitting closure: the drop notifies the capacity signal, which is shared by
/// the whole runtime, while the admission guard is still alive. A chunk rejected
/// before its lease is built does not reach it — the first version of this test
/// asserted that path and found nothing, which is why the one below drives the
/// drain rejection instead.
#[test]
fn a_rejected_control_drops_its_lease_on_a_leaf_under_the_admission_lock() {
    let h = Harness::standard();
    h.owner
        .notify_output_drained(pty_runtime_domain::process::DrainOutcome::Eof);
    tier::reset();

    // The lease is reserved first, then the closure refuses because the reader
    // has drained, so the lease is dropped while admission is still held.
    let size = pty_runtime_domain::terminal::TerminalSize::new(80, 24).unwrap();
    assert!(h.owner.resize(size).is_err());

    let seen = tier::observed_nestings();
    assert!(
        seen.contains(&(Tier::Admission, Tier::Leaf)),
        "the rejected control's lease notified a leaf under admission; saw {seen:?}",
    );
    assert!(tier::holds_none());
}

/// Park, restore, transfer and close each drive different lock paths. None may
/// invert, and between them they must exercise both documented nestings.
#[test]
fn the_full_park_restore_and_close_cycle_never_inverts() {
    let h = Harness::standard();
    tier::reset();
    h.owner.stage_output(b"a");
    h.pump();
    h.park();

    // Restoring takes the workspace, then reaches storage and admission.
    h.owner.stage_output(b"b");
    h.pump();
    assert_eq!(
        h.owner.status().residency,
        crate::projection::Residency::Resident
    );

    h.close();
    let seen = tier::observed_nestings();
    assert!(
        seen.contains(&(Tier::Workspace, Tier::Admission)),
        "saw {seen:?}",
    );
    assert!(tier::holds_none());
}

/// Callers and the worker run concurrently, which is the situation the order
/// exists to make safe. Any thread that inverted would panic in its own stack,
/// so a clean join is the evidence.
#[test]
fn concurrent_callers_and_worker_agree_on_the_order() {
    let h = Arc::new(Harness::standard());
    let start = Arc::new(Barrier::new(3));

    let worker = {
        let h = Arc::clone(&h);
        let start = Arc::clone(&start);
        std::thread::spawn(move || {
            tier::reset();
            start.wait();
            for _ in 0..64 {
                h.step();
                h.jobs.run_one();
            }
            assert!(tier::holds_none());
            tier::observed_nestings()
        })
    };
    let writer = {
        let h = Arc::clone(&h);
        let start = Arc::clone(&start);
        std::thread::spawn(move || {
            tier::reset();
            start.wait();
            for _ in 0..64 {
                let _ = h.owner.stage_output(b"x");
                let _ = h.owner.status();
            }
            assert!(tier::holds_none());
        })
    };

    start.wait();
    for _ in 0..64 {
        // Observation and control admission from a third thread.
        drop(h.owner.view());
        let _ = h.budgets.unreclaimed_sources();
    }

    let worker_nestings = worker.join().expect("worker thread must not invert");
    writer.join().expect("writer thread must not invert");
    assert!(
        worker_nestings.contains(&(Tier::Workspace, Tier::Admission)),
        "the worker thread exercised the documented nesting; saw {worker_nestings:?}",
    );
    h.jobs.run_one();
}

/// Cleanup releases collaborators in a fixed order while holding the workspace.
/// It is the densest lock path in the codebase, so it gets its own check.
#[test]
fn teardown_releases_collaborators_without_inverting() {
    let h = Harness::standard();
    h.owner.stage_output(b"a");
    h.pump();
    tier::reset();
    h.owner.finish_after_shutdown();
    assert!(tier::holds_none());
    assert!(h.owner.wiring.services().is_err());
    let seen = tier::observed_nestings();
    assert!(
        seen.iter().all(|&(outer, inner)| outer < inner),
        "every nesting during teardown went downward; saw {seen:?}",
    );
}
