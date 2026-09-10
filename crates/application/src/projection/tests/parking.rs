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
