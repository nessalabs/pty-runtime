use super::support::*;
use crate::projection::*;
use pty_runtime_domain::{SessionLifetime, process::DrainOutcome, terminal::TerminalSize};
use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Wake, Waker},
};
fn open_observer(h: &Harness) -> (PinnedCheckpoint, TransferObserver) {
    let mut operation = h.owner.begin_transfer().unwrap();
    h.pump();
    result(&mut operation).unwrap().into_parts()
}
fn event(observer: &TransferObserver, cursor: TransferCursor) -> TransferEvent {
    match observer.read(cursor).unwrap() {
        TransferRead::Event(event) => event,
        _ => panic!("expected event"),
    }
}
#[test]
fn snapshot_boundary_and_independent_byte_control_order_are_exact() {
    let h = Harness::standard();
    h.owner.stage_output(b"before");
    let mut transfer = h.owner.begin_transfer().unwrap();
    h.owner.stage_output(b"after");
    let mut resized = h.owner.resize(TerminalSize::new(3, 1).unwrap()).unwrap();
    h.owner.stage_output(b"last");
    h.pump();
    result(&mut resized).unwrap();
    let (pin, first) = result(&mut transfer).unwrap().into_parts();
    let mut cursor = first.boundary().cursor;
    assert_eq!(cursor.sequence, 1);
    assert_eq!(pin.checkpoint().descriptor.processed.offset, 6);
    assert_eq!(
        first.boundary().processed,
        pin.checkpoint().descriptor.processed
    );
    let output = event(&first, cursor);
    assert!(matches!(output.kind(), TransferEventKind::Output(b"after")));
    cursor = output.after().cursor;
    let resize = event(&first, cursor);
    assert!(matches!(
        resize.kind(),
        TransferEventKind::Resize { generation: 1, .. }
    ));
    assert_eq!(resize.after().processed, output.after().processed);
    let last = event(&first, resize.after().cursor);
    assert!(matches!(last.kind(), TransferEventKind::Output(b"last")));
    assert_eq!(last.after().control_generation, 1);
    assert!(matches!(
        first.read(last.after().cursor),
        Ok(TransferRead::Pending)
    ));
    assert!(matches!(
        first.read(first.boundary().cursor),
        Ok(TransferRead::Event(_))
    ));
    h.close();
    assert!(matches!(first.read(cursor), Err(TransferError::Closed)));
}
#[test]
fn retained_payload_and_control_slots_force_explicit_holes_until_consumers_drop() {
    let mut opts = options();
    opts.journal_slots = 1;
    opts.journal_bytes = 16;
    let h = Harness::new(opts, ProjectionLimits::default());
    let (_pin, first) = open_observer(&h);
    h.owner.stage_output(b"held");
    h.pump();
    let held = event(&first, first.boundary().cursor);
    let mut resize = h.owner.resize(TerminalSize::new(3, 1).unwrap()).unwrap();
    h.pump();
    result(&mut resize).unwrap();
    assert!(matches!(
        first.read(held.after().cursor),
        Err(TransferError::ResyncRequired { .. })
    ));
    assert_eq!(h.owner.status().control_generation, 1);
    // Final observer drop cannot release the consumer-held record's slot or bytes.
    drop(first);
    assert!(
        !h.budgets
            .journal_bytes
            .acquire(h.budgets.limits.journal_bytes)
    );
    drop(held);
    assert!(
        h.budgets
            .journal_bytes
            .acquire(h.budgets.limits.journal_bytes)
    );
    h.budgets
        .journal_bytes
        .release(h.budgets.limits.journal_bytes);
    let (_pin, second) = open_observer(&h);
    h.owner.stage_output(b"next");
    h.pump();
    assert!(matches!(
        event(&second, second.boundary().cursor).kind(),
        TransferEventKind::Output(b"next")
    ));
    h.close();
}
#[test]
fn foreign_future_and_evicted_cursors_are_distinct_and_readers_are_independent() {
    let mut opts = options();
    opts.journal_slots = 1;
    let h = Harness::new(opts, ProjectionLimits::default());
    let (_, first) = open_observer(&h);
    let (_, second) = open_observer(&h);
    let cursor = first.boundary().cursor;
    assert!(matches!(
        first.read(TransferCursor {
            lifetime: SessionLifetime::new(9, 2),
            ..cursor
        }),
        Err(TransferError::ForeignLifetime)
    ));
    assert!(matches!(
        first.read(TransferCursor {
            sequence: 1,
            ..cursor
        }),
        Err(TransferError::FutureCursor)
    ));
    h.owner.stage_output(b"one");
    h.pump();
    let tail = event(&first, cursor).after().cursor;
    assert!(matches!(
        event(&second, cursor).kind(),
        TransferEventKind::Output(b"one")
    ));
    h.owner.stage_output(b"two");
    h.pump();
    assert!(matches!(
        first.read(cursor),
        Err(TransferError::ResyncRequired { .. })
    ));
    assert!(matches!(
        event(&second, tail).kind(),
        TransferEventKind::Output(b"two")
    ));
    h.close();
}
struct CountWake(AtomicUsize);
impl Wake for CountWake {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}
#[test]
fn cancelled_wait_removes_waker_and_final_observer_releases_retention() {
    let h = Harness::standard();
    let (_, mut observer) = open_observer(&h);
    let count = Arc::new(CountWake(AtomicUsize::new(0)));
    let waker = Waker::from(count.clone());
    let cursor = observer.boundary().cursor;
    let mut wait = observer.wait(cursor);
    assert!(
        std::pin::Pin::new(&mut wait)
            .poll(&mut Context::from_waker(&waker))
            .is_pending()
    );
    drop(wait);
    h.owner.stage_output(b"one");
    h.pump();
    assert_eq!(count.0.load(Ordering::Relaxed), 0);
    drop(observer);
    assert!(
        h.budgets
            .journal_bytes
            .acquire(h.budgets.limits.journal_bytes)
    );
    h.budgets
        .journal_bytes
        .release(h.budgets.limits.journal_bytes);
    h.close();
}
#[test]
fn parked_transfer_precedes_staged_mutation_and_rejected_io_releases_observer_permit() {
    let mut opts = options();
    opts.transfer_observers = 1;
    let h = Harness::new(opts, ProjectionLimits::default());
    h.park();
    h.jobs.reject.store(true, Ordering::Release);
    let mut rejected = h.owner.begin_transfer().unwrap();
    h.pump();
    assert!(result(&mut rejected).is_err());
    drop(rejected);
    h.jobs.reject.store(false, Ordering::Release);
    let mut accepted = h.owner.begin_transfer().unwrap();
    h.step();
    assert_eq!(h.probe.alive.load(Ordering::Acquire), 0);
    h.owner.stage_output(b"after");
    h.jobs.one();
    h.pump();
    let (pin, observer) = result(&mut accepted).unwrap().into_parts();
    assert_eq!(pin.checkpoint().descriptor.processed.offset, 0);
    assert!(matches!(
        event(&observer, observer.boundary().cursor).kind(),
        TransferEventKind::Output(b"after")
    ));
    h.close();
}
#[test]
fn drain_is_sealed_but_end_waits_for_parser_and_failure_ends_only_the_valid_prefix() {
    let h = Harness::standard();
    let (_, observer) = open_observer(&h);
    h.owner.stage_output(b"last");
    h.owner.notify_output_drained(DrainOutcome::Eof);
    assert!(matches!(
        observer.read(observer.boundary().cursor),
        Ok(TransferRead::Pending)
    ));
    assert!(h.owner.resize(TerminalSize::new(3, 1).unwrap()).is_err());
    h.pump();
    let event = event(&observer, observer.boundary().cursor);
    match observer.read(event.after().cursor).unwrap() {
        TransferRead::End(end) => {
            assert_eq!(end.drain, Some(DrainOutcome::Eof));
            assert_eq!(end.projection_failure, None);
        }
        _ => panic!("missing drained end"),
    }
    h.close();
    let h = Harness::standard();
    let (_, observer) = open_observer(&h);
    h.owner.fail(ProjectionError::Worker);
    match observer.read(observer.boundary().cursor).unwrap() {
        TransferRead::End(end) => {
            assert_eq!(end.drain, None);
            assert_eq!(end.projection_failure, Some(ProjectionError::Worker));
        }
        _ => panic!("failure hung continuation"),
    }
    h.close();
}

#[test]
fn observer_admission_cancel_and_snapshot_pins_release_independently() {
    let mut opts = options();
    opts.transfer_observers = 1;
    let h = Harness::new(opts, ProjectionLimits::default());
    let cancelled = h.owner.begin_transfer().unwrap();
    assert!(matches!(
        h.owner.begin_transfer(),
        Err(ProjectionError::Capacity)
    ));
    drop(cancelled);
    h.pump();
    let (pin, observer) = open_observer(&h);
    assert!(matches!(
        h.owner.begin_transfer(),
        Err(ProjectionError::Capacity)
    ));
    drop(observer);
    let (second_pin, observer) = open_observer(&h);
    // A snapshot pin alone does not retain the observer quota; the reverse also holds.
    drop(pin);
    drop(second_pin);
    assert!(
        h.budgets
            .checkpoints
            .acquire(h.budgets.limits.checkpoint_bytes)
    );
    h.budgets
        .checkpoints
        .release(h.budgets.limits.checkpoint_bytes);
    drop(observer);
    h.close();
}
#[test]
fn no_observers_allocate_no_continuation_storage_and_new_transfer_skips_no_applied_state() {
    let h = Harness::standard();
    h.owner.stage_output(b"past");
    h.pump();
    assert!(
        h.budgets
            .journal_bytes
            .acquire(h.budgets.limits.journal_bytes)
    );
    assert!(
        h.budgets
            .journal_slots
            .acquire(h.budgets.limits.journal_slots)
    );
    h.budgets
        .journal_bytes
        .release(h.budgets.limits.journal_bytes);
    h.budgets
        .journal_slots
        .release(h.budgets.limits.journal_slots);
    let (pin, observer) = open_observer(&h);
    assert_eq!(observer.boundary().cursor.sequence, 1);
    assert_eq!(pin.checkpoint().descriptor.processed.offset, 4);
    assert!(matches!(
        observer.read(observer.boundary().cursor),
        Ok(TransferRead::Pending)
    ));
    h.close();
}
