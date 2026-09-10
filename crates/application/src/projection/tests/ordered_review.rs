//! Independent regressions for concurrent End publication and arbitrary waker drop.
use super::support::*;
use crate::{projection::*, scheduling::IScheduledWork, terminal::ITerminal};
use pty_runtime_domain::terminal::*;
use std::{
    sync::{Arc, Barrier},
    task::{Context, Poll, Wake, Waker},
    time::Duration,
};

struct CompletesTicketOnDrop(Arc<crate::projection::observation::Ticket<()>>);
impl Drop for CompletesTicketOnDrop {
    fn drop(&mut self) {
        self.0.complete(Ok(()));
    }
}
// Cancellation runs this caller-owned destructor, which reenters the same ticket.
#[allow(clippy::manual_noop_waker)]
impl Wake for CompletesTicketOnDrop {
    fn wake(self: Arc<Self>) {}
}
#[test]
fn cancelled_projection_wait_drops_reentrant_waker_outside_ticket_lock() {
    let (ticket, mut operation) = crate::projection::observation::Ticket::<()>::new(None);
    let waker = Waker::from(Arc::new(CompletesTicketOnDrop(ticket)));
    assert!(
        operation
            .as_mut()
            .poll(&mut Context::from_waker(&waker))
            .is_pending()
    );
    drop(waker);
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        drop(operation);
        sender.send(()).unwrap();
    });
    assert_eq!(receiver.recv_timeout(Duration::from_secs(2)), Ok(()));
}

fn observer(h: &Harness) -> TransferObserver {
    let mut opening = h.owner.begin_transfer().unwrap();
    h.pump();
    result(&mut opening).unwrap().into_parts().1
}
struct PausedFeed {
    inner: Box<dyn ITerminal>,
    barrier: Arc<Barrier>,
}
impl ITerminal for PausedFeed {
    fn history(&mut self, start: u64, _count: u16) -> Result<TerminalHistory, TerminalError> {
        // These doubles model feed and control ordering, not retained rows.
        Ok(TerminalHistory {
            start,
            cols: 0,
            cells: Vec::new(),
            total: 0,
            scrollback: 0,
        })
    }

    fn feed(&mut self, bytes: &[u8]) -> Result<TerminalEffects, TerminalError> {
        self.barrier.wait();
        self.barrier.wait();
        self.inner.feed(bytes)
    }
    fn resize(
        &mut self,
        size: TerminalSize,
        generation: ControlGeneration,
    ) -> Result<(), TerminalError> {
        self.inner.resize(size, generation)
    }
    fn view(&mut self) -> Result<TerminalView, TerminalError> {
        self.inner.view()
    }
    fn checkpoint(
        &mut self,
        descriptor: CheckpointDescriptor,
    ) -> Result<TerminalCheckpoint, TerminalError> {
        self.inner.checkpoint(descriptor)
    }
    fn restoration_progress(&self) -> RestorationProgress {
        self.inner.restoration_progress()
    }
    fn restore_history_step(&mut self) -> Result<RestorationProgress, TerminalError> {
        self.inner.restore_history_step()
    }
    fn compress_history_step(&mut self) -> Result<bool, TerminalError> {
        self.inner.compress_history_step()
    }
}
#[test]
fn failed_end_stays_immutable_when_an_inflight_native_feed_returns_success() {
    let h = Harness::standard();
    let observer = observer(&h);
    let boundary = observer.boundary();
    let barrier = Arc::new(Barrier::new(2));
    {
        let mut workspace = h.owner.workspace.lock().unwrap();
        let inner = workspace.terminal.take().unwrap();
        workspace.terminal = Some(Box::new(PausedFeed {
            inner,
            barrier: barrier.clone(),
        }));
    }
    h.owner.stage_output(b"late");
    let owner = h.owner.clone();
    let worker = std::thread::spawn(move || owner.run());
    barrier.wait();
    // A rejected external scheduler wake can report failure during an admitted feed.
    h.owner.fail(ProjectionError::Worker);
    match observer.read(boundary.cursor).unwrap() {
        TransferRead::End(end) => assert_eq!(end.boundary, boundary),
        _ => panic!("failed valid prefix was not sealed"),
    }
    barrier.wait();
    worker.join().unwrap();
    match observer.read(boundary.cursor).unwrap() {
        TransferRead::End(end) => {
            assert_eq!(end.boundary, boundary);
            assert_eq!(end.projection_failure, Some(ProjectionError::Worker));
        }
        _ => panic!("late in-flight success published a mutation beyond immutable End"),
    }
    h.close();
}
struct OwnsObserver {
    _observer: TransferObserver,
}
#[test]
fn observation_requests_do_not_delay_the_final_applied_prefix() {
    let h = Harness::standard();
    let observer = observer(&h);
    let cursor = observer.boundary().cursor;
    h.owner
        .notify_output_drained(pty_runtime_domain::process::DrainOutcome::Eof);
    // Keep observation work queued on every scheduler turn after final drain.
    // These operations cannot mutate the checkpoint's continuation boundary.
    for _ in 0..8 {
        let mut view = h.owner.view().unwrap();
        h.owner.run();
        result(&mut view).unwrap();
    }
    assert!(matches!(observer.read(cursor), Ok(TransferRead::End(_))));
    h.close();
}
#[test]
fn parked_snapshot_io_does_not_delay_an_existing_observers_end() {
    let h = Harness::standard();
    let observer = observer(&h);
    let cursor = observer.boundary().cursor;
    h.park();
    let mut snapshot = h.owner.begin_transfer().unwrap();
    h.step();
    assert!(matches!(
        h.owner.workspace.lock().unwrap().io.as_ref(),
        Some(crate::projection::state::PendingIo::Transfer { .. })
    ));
    h.owner
        .notify_output_drained(pty_runtime_domain::process::DrainOutcome::Eof);
    // The executor job remains queued: no provider read result exists yet.
    h.step();
    assert!(matches!(observer.read(cursor), Ok(TransferRead::End(_))));
    assert!(h.jobs.run_one());
    h.pump();
    result(&mut snapshot).unwrap();
    h.close();
}
// This waker's owned observer destructor is the behavior under test.
#[allow(clippy::manual_noop_waker)]
impl Wake for OwnsObserver {
    fn wake(self: Arc<Self>) {}
}
#[test]
fn replacing_a_pending_waker_drops_its_observer_outside_the_journal_lock() {
    let h = Harness::standard();
    let mut first = observer(&h);
    let second = observer(&h);
    let cursor = first.boundary().cursor;
    let waker = Waker::from(Arc::new(OwnsObserver { _observer: second }));
    assert!(matches!(
        first.poll_read(cursor, &mut Context::from_waker(&waker)),
        Poll::Pending
    ));
    drop(waker);
    // The registration now owns the only waker. Clearing it drops another
    // same-journal observer, whose destructor legitimately deregisters itself.
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let pending = matches!(first.read(cursor), Ok(TransferRead::Pending));
        sender.send(pending).unwrap();
    });
    assert_eq!(
        receiver.recv_timeout(Duration::from_secs(2)),
        Ok(true),
        "reentrant waker destruction must not hold the journal mutex"
    );
    h.close();
}
