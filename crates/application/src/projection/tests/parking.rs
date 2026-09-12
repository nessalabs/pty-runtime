use super::{support::*, terminal::Trace};
use crate::{process::OutputAcceptance, projection::Residency, scheduling::IScheduledWork};
use pty_runtime_domain::terminal::ControlGeneration;
use pty_runtime_domain::terminal::TerminalSize;
use std::sync::{Arc, Barrier, atomic::Ordering};
#[test]
fn idle_park_restore_orders_bytes_resize_history_and_single_authoritative_reply() {
    let h = Harness::standard();
    assert_eq!(h.owner.stage_output(b"ab"), OutputAcceptance::Accepted);
    let mut resize = h.owner.resize(TerminalSize::new(3, 1).unwrap()).unwrap();
    h.pump();
    assert!(result(&mut resize).unwrap().model.is_ok());
    h.park();
    assert_eq!(h.probe.alive.load(Ordering::Acquire), 0);
    let stored = h
        .store
        .entries
        .lock()
        .unwrap()
        .values()
        .next()
        .unwrap()
        .1
        .clone();
    assert_eq!(stored.descriptor.processed.offset, 2);
    assert_eq!(
        stored.descriptor.control_generation,
        ControlGeneration::from_raw(1)
    );
    assert_eq!(h.owner.stage_output(b"?c"), OutputAcceptance::Accepted);
    h.pump();
    assert_eq!(h.owner.status().processed.offset, 4);
    assert_eq!(*h.process.writes.lock().unwrap(), vec![b"R".to_vec()]);
    let trace = h.probe.trace.lock().unwrap().clone();
    let restore = trace
        .iter()
        .position(|t| matches!(t, Trace::Restore(2, 1)))
        .unwrap();
    let feed = trace
        .iter()
        .position(|t| matches!(t,Trace::Feed(v) if v==b"?c"))
        .unwrap();
    assert!(matches!(trace[restore + 1], Trace::History(1)));
    assert!(matches!(trace[restore + 2], Trace::History(0)));
    assert!(feed > restore + 2);
    let mut view = h.owner.view().unwrap();
    h.pump();
    assert_eq!(result(&mut view).unwrap().view().cells[0].text, "ab?c");
    h.close();
}
#[test]
fn output_during_encode_is_bounded_and_stale_commit_never_releases_live_model() {
    let h = Harness::standard();
    h.owner.stage_output(b"a");
    h.pump();
    h.clock.0.store(60, Ordering::Release);
    let barrier = Arc::new(Barrier::new(2));
    *h.probe.encode_barrier.lock().unwrap() = Some(barrier.clone());
    let owner = h.owner.clone();
    let worker = std::thread::spawn(move || owner.run());
    barrier.wait();
    assert_eq!(h.owner.stage_output(b"b"), OutputAcceptance::Accepted);
    assert_eq!(h.owner.status().published.offset, 2);
    barrier.wait();
    worker.join().unwrap();
    assert_eq!(h.probe.alive.load(Ordering::Acquire), 1);
    assert_eq!(h.jobs.len(), 1);
    h.jobs.run_one();
    h.pump();
    assert_eq!(h.owner.status().residency, Residency::Resident);
    assert_eq!(h.owner.status().processed.offset, 2);
    assert_eq!(h.store.deletes.load(Ordering::Acquire), 1);
    assert!(h.store.entries.lock().unwrap().is_empty());
    h.close();
}
#[test]
fn output_during_commit_retains_model_and_cleanup_does_not_block_feed() {
    let h = Harness::standard();
    h.owner.stage_output(b"a");
    h.pump();
    h.clock.0.store(60, Ordering::Release);
    h.step();
    assert_eq!(h.jobs.len(), 1);
    assert_eq!(h.probe.alive.load(Ordering::Acquire), 1);
    h.owner.stage_output(b"b");
    h.step();
    assert_eq!(h.owner.status().processed.offset, 2);
    h.jobs.run_one();
    h.step();
    assert_eq!(h.jobs.len(), 1); // stale-source deletion queued
    h.owner.stage_output(b"c");
    h.step();
    assert_eq!(h.owner.status().processed.offset, 3);
    assert_eq!(h.store.entries.lock().unwrap().len(), 1);
    h.jobs.run_one();
    h.pump();
    h.close();
}
#[test]
fn parked_transfer_pin_uses_saved_source_without_native_restore() {
    let h = Harness::standard();
    h.owner.stage_output(b"state");
    h.pump();
    h.park();
    let mut checkpoint = h.owner.checkpoint().unwrap();
    h.pump();
    let pin = result(&mut checkpoint).unwrap();
    assert_eq!(pin.checkpoint().bytes, b"state");
    assert_eq!(pin.checkpoint().descriptor.processed.offset, 5);
    assert_eq!(h.owner.status().residency, Residency::Parked);
    assert_eq!(h.probe.alive.load(Ordering::Acquire), 0);
    assert!(
        !h.probe
            .trace
            .lock()
            .unwrap()
            .iter()
            .any(|t| matches!(t, Trace::Restore(..)))
    );
    h.close();
    // The leased immutable result outlives closure without any native/provider dependency.
    assert_eq!(pin.checkpoint().bytes, b"state");
}

#[test]
fn ready_view_captures_its_own_history_and_exact_order_boundary() {
    let h = Harness::standard();
    h.owner.stage_output(b"old");
    h.pump();
    h.park();
    let mut view = h.owner.view().unwrap();
    h.step();
    h.jobs.run_one();
    h.step();
    let view = result(&mut view).unwrap();
    assert_eq!(view.processed().offset, 3);
    assert_eq!(view.control_generation(), ControlGeneration::from_raw(0));
    assert_eq!(
        view.restoration_progress(),
        pty_runtime_domain::terminal::RestorationProgress::Usable
    );
    h.owner.stage_output(b"new");
    h.pump();
    assert_eq!(h.owner.status().processed.offset, 6);
    assert_eq!(view.processed().offset, 3);
    assert_eq!(view.view().cells[0].text, "old");
    h.close();
}

/// The worker must not start a park while native control work is still in
/// flight, because `commit_park` releases the live model on the assumption that
/// nothing is using it.
///
/// This replaces an `engine_idle` parameter on `commit_park` that was always
/// passed `true`. A condition the worker cannot produce is not defence — nothing
/// can test it, and a coverage gate cannot tell it from dead code. What actually
/// protects the release is the ordering asserted here: `serve` is reached only
/// after `poll_inflight_operations` returns `None`, so a pending resize keeps the
/// park from ever starting, however long the park has been due.
#[test]
fn the_worker_never_parks_while_native_work_is_in_flight() {
    let h = Harness::standard();
    h.process.hold_resize.store(true, Ordering::Release);
    assert_eq!(h.owner.stage_output(b"ab"), OutputAcceptance::Accepted);
    let mut resize = h.owner.resize(TerminalSize::new(3, 1).unwrap()).unwrap();

    // Well past the park deadline, with the resize still outstanding.
    h.clock.0.store(600, Ordering::Release);
    h.pump();
    assert!(matches!(poll(&mut resize), std::task::Poll::Pending));
    assert_eq!(
        h.owner.status().residency,
        Residency::Resident,
        "a park started while a resize was in flight"
    );
    assert_eq!(h.store.commits.load(Ordering::Acquire), 0);
    assert_eq!(h.jobs.len(), 0, "no commit job should have been submitted");
    assert_eq!(h.probe.alive.load(Ordering::Acquire), 1);

    // Once it settles, the overdue park runs immediately.
    h.process.hold_resize.store(false, Ordering::Release);
    h.pump();
    assert!(result(&mut resize).unwrap().model.is_ok());
    assert_eq!(h.owner.status().residency, Residency::Parked);
    assert_eq!(h.store.commits.load(Ordering::Acquire), 1);
    h.close();
}

/// A resize admitted while a park commit is in flight must not let that commit
/// release the live model.
///
/// The interleaving is real: `serve` applies queued commands while the commit
/// job is still outstanding, so `workspace.resize` can be `Some` by the time
/// `finish_io` lands the commit. Releasing there would drop the terminal out
/// from under a pending OS resize, and the next poll would report the model
/// resize as `Closed` and fail the projection even though the child resized
/// fine.
#[test]
fn a_resize_admitted_during_a_commit_blocks_the_release() {
    let h = Harness::standard();
    h.process.hold_resize.store(true, Ordering::Release);
    assert_eq!(h.owner.stage_output(b"ab"), OutputAcceptance::Accepted);
    h.pump();

    // Park becomes due; start the commit but leave the job unfinished.
    h.clock.0.store(60, Ordering::Release);
    h.step();
    assert_eq!(h.jobs.len(), 1, "a commit job should be outstanding");
    assert_eq!(h.owner.status().residency, Residency::Parking);

    // Admit a resize and let the worker hand it to the child while the commit
    // is still in flight.
    let target = TerminalSize::new(4, 3).unwrap();
    let mut resize = h.owner.resize(target).unwrap();
    h.step();
    assert_eq!(
        h.process.controls.lock().unwrap().len(),
        1,
        "the resize should have reached the child"
    );

    // Now land the commit.
    assert!(h.jobs.run_one());
    h.step();

    assert_eq!(
        h.owner.status().residency,
        Residency::Resident,
        "the commit must not park while a resize is outstanding"
    );
    assert_eq!(h.owner.status().failure, None);
    assert_eq!(
        h.probe.alive.load(Ordering::Acquire),
        1,
        "the live model must survive the commit"
    );

    // The resize then completes against a model that is still there.
    h.process.hold_resize.store(false, Ordering::Release);
    h.pump();
    let outcome = result(&mut resize).unwrap();
    assert_eq!(outcome.os, Ok(()));
    assert_eq!(outcome.model, Ok(()), "the model resize must have applied");
    assert_eq!(h.owner.status().failure, None);
    drop(resize);
    h.close();
}
