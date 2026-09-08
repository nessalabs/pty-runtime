use super::{support::*, terminal::Trace};
use crate::{process::OutputAcceptance, projection::*};
use pty_runtime_domain::{process::DrainOutcome, terminal::*};
use std::sync::atomic::Ordering;

#[test]
fn ready_interleaves_live_bytes_and_controls_without_releasing_source_before_finish() {
    let h = Harness::standard();
    h.probe.live_restore.store(true, Ordering::Release);
    h.owner.stage_output(b"before");
    h.pump();
    h.park();
    assert_eq!(h.owner.stage_output(b"?after"), OutputAcceptance::Accepted);
    let mut resize = h.owner.resize(TerminalSize::new(3, 1).unwrap()).unwrap();
    h.step(); // source read admitted
    h.jobs.one();
    h.step(); // READY, then first live feed before any history
    assert_eq!(h.owner.status().processed.offset, 12);
    assert_eq!(h.owner.status().history, RestorationProgress::Usable);
    assert!(h.owner.engine.lock().unwrap().source.is_some());
    assert!(h.owner.engine.lock().unwrap().restore_memory.is_some());
    assert_eq!(h.store.entries.lock().unwrap().len(), 1);
    h.step(); // reply completes; one history unit
    assert_eq!(*h.process.writes.lock().unwrap(), vec![b"R".to_vec()]);
    assert!(h.owner.engine.lock().unwrap().source.is_some());
    h.step(); // resize OS operation admitted
    assert!(poll(&mut resize).is_pending());
    h.step(); // resize completed, then FINISH
    assert!(result(&mut resize).unwrap().model.is_ok());
    assert_eq!(h.owner.status().history, RestorationProgress::Complete);
    assert!(h.owner.engine.lock().unwrap().source.is_none());
    assert!(h.owner.engine.lock().unwrap().restore_memory.is_none());
    let trace = h.probe.trace.lock().unwrap().clone();
    let start = trace
        .iter()
        .position(|event| matches!(event, Trace::Restore(..)))
        .unwrap();
    assert_eq!(
        &trace[start + 1..],
        &[
            Trace::Feed(b"?after".to_vec()),
            Trace::History(1),
            Trace::Resize(1),
            Trace::History(0),
        ]
    );
    h.pump();
    assert!(h.store.entries.lock().unwrap().is_empty());
    h.close();
}

#[test]
fn skipped_history_is_visible_and_lifetime_count_survives_the_next_park() {
    let h = Harness::standard();
    h.probe.live_restore.store(true, Ordering::Release);
    h.probe.skip_history.store(true, Ordering::Release);
    h.park();
    h.owner.stage_output(b"live");
    h.pump();
    assert_eq!(
        h.owner.status().history,
        RestorationProgress::FinishedWithSkippedHistory { skipped_pages: 2 }
    );
    assert_eq!(h.owner.status().skipped_history_pages, 2);
    let mut view = h.owner.view().unwrap();
    h.pump();
    assert_eq!(
        result(&mut view).unwrap().restoration_progress(),
        h.owner.status().history
    );
    h.clock.0.store(120, Ordering::Release);
    h.pump();
    assert_eq!(h.owner.status().residency, Residency::Parked);
    h.probe.skip_history.store(false, Ordering::Release);
    h.owner.stage_output(b"next");
    h.pump();
    assert_eq!(h.owner.status().history, RestorationProgress::Complete);
    assert_eq!(h.owner.status().skipped_history_pages, 2);
    h.close();
}

#[test]
fn drain_end_waits_for_history_validation_and_retains_corrupt_source() {
    let h = Harness::standard();
    h.probe.live_restore.store(true, Ordering::Release);
    let mut transfer = h.owner.begin_transfer().unwrap();
    h.pump();
    let (_, observer) = result(&mut transfer).unwrap().into_parts();
    h.park();
    h.owner.stage_output(b"live");
    h.owner.notify_output_drained(DrainOutcome::Eof);
    h.step();
    h.jobs.one();
    h.step();
    let event = match observer.read(observer.boundary().cursor).unwrap() {
        TransferRead::Event(event) => event,
        _ => panic!("expected first applied live bytes"),
    };
    assert!(matches!(
        observer.read(event.after().cursor),
        Ok(TransferRead::Pending)
    ));
    h.probe.fail_history.store(true, Ordering::Release);
    h.step();
    match observer.read(event.after().cursor).unwrap() {
        TransferRead::End(end) => assert_eq!(
            end.projection_failure,
            Some(ProjectionError::Terminal(TerminalError::CorruptCheckpoint))
        ),
        _ => panic!("expected failure end"),
    }
    assert!(h.owner.engine.lock().unwrap().source.is_some());
    assert_eq!(h.store.entries.lock().unwrap().len(), 1);
    h.close();
}
