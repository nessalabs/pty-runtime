//! What happens to in-flight native work when the things around it refuse.
//!
//! A generated reply and a resize are the two operations a projection hands to
//! the child process, and each has to survive a refusal from three different
//! directions: the process is gone, the runtime-wide input quota is full, or the
//! process itself rejects the write. The distinction that matters throughout is
//! between "try again shortly", which must keep the work, and "this projection
//! is broken", which must not silently lose it either.
use super::{support::*, terminal::Trace};
use crate::{
    process::OutputAcceptance,
    projection::{ProjectionError, ProjectionLimits, Residency},
    scheduling::WorkSchedule,
};
use pty_runtime_domain::terminal::ControlGeneration;
use pty_runtime_domain::{process::ProcessError, terminal::TerminalSize};
use std::{sync::atomic::Ordering, time::Duration};

/// Feed a chunk that makes the engine generate a reply, stopping before the
/// worker hands it to the process.
fn staged_reply(h: &Harness) {
    assert_eq!(h.owner.stage_output(b"?"), OutputAcceptance::Accepted);
    h.step();
    assert!(
        h.owner.workspace.lock().unwrap().reply.is_some(),
        "the engine should have produced a reply to send"
    );
}
fn retry_soon(schedule: WorkSchedule) -> bool {
    matches!(schedule, WorkSchedule::After(delay) if delay > Duration::ZERO)
}

#[test]
fn resize_admission_and_completion_errors_preserve_model_and_allow_same_generation_retry() {
    for admission in [true, false] {
        let h = Harness::standard();
        h.process
            .fail_resize_admission
            .store(admission, Ordering::Release);
        h.process
            .fail_resize_completion
            .store(!admission, Ordering::Release);
        let target = TerminalSize::new(4, 3).unwrap();
        let mut resize = h.owner.resize(target).unwrap();
        h.pump();
        let outcome = result(&mut resize).unwrap();
        assert_eq!(outcome.generation, ControlGeneration::from_raw(1));
        assert_eq!(outcome.os, Err(ProcessError::Io));
        assert_eq!(
            outcome.model,
            Err(ProjectionError::Process(ProcessError::Io))
        );
        drop(resize);
        assert_eq!(h.owner.status().residency, Residency::Resident);
        assert_eq!(h.owner.status().failure, None);
        assert_eq!(
            h.owner.status().control_generation,
            ControlGeneration::from_raw(0)
        );
        assert_eq!(h.budgets.resources().staging_slots.used, 0);
        assert_eq!(h.budgets.resources().requests.used, 0);
        assert!(
            !h.probe
                .trace
                .lock()
                .unwrap()
                .iter()
                .any(|t| matches!(t, Trace::Resize(_)))
        );
        let mut view = h.owner.view().unwrap();
        h.pump();
        assert_eq!(
            result(&mut view).unwrap().view().size,
            options().terminal.size
        );
        drop(view);
        h.process
            .fail_resize_admission
            .store(false, Ordering::Release);
        h.process
            .fail_resize_completion
            .store(false, Ordering::Release);
        let mut retry = h.owner.resize(target).unwrap();
        h.pump();
        let outcome = result(&mut retry).unwrap();
        assert_eq!(outcome.generation, ControlGeneration::from_raw(1));
        assert_eq!(outcome.os, Ok(()));
        assert_eq!(outcome.model, Ok(()));
        assert_eq!(
            h.owner.status().control_generation,
            ControlGeneration::from_raw(1)
        );
        let mut view = h.owner.view().unwrap();
        h.pump();
        assert_eq!(result(&mut view).unwrap().view().size, target);
        drop((retry, view));
        h.close();
    }
}

/// An unbound process is a wait, not a failure: the reply is kept for whenever
/// a child is bound, and the projection stays usable.
#[test]
fn a_reply_with_no_bound_process_is_retained_rather_than_failed() {
    let h = Harness::standard();
    staged_reply(&h);
    h.owner.wiring.take_process();

    assert!(matches!(h.step(), WorkSchedule::Dormant));
    assert_eq!(h.owner.status().failure, None);
    assert!(
        h.owner.workspace.lock().unwrap().reply.is_some(),
        "the reply must survive having nowhere to go"
    );
    assert!(h.process.writes.lock().unwrap().is_empty());
}

/// A full runtime-wide input quota is another session's doing, so the reply
/// waits for it rather than failing this projection.
#[test]
fn a_full_input_quota_defers_the_reply_and_the_retry_sends_it() {
    let h = Harness::standard();
    staged_reply(&h);
    assert!(h.input_bytes.acquire(128), "fixture quota is 128 bytes");

    assert!(retry_soon(h.step()), "a full input quota asks for a retry");
    assert_eq!(h.owner.status().failure, None);
    assert!(h.process.writes.lock().unwrap().is_empty());

    h.input_bytes.release(128);
    h.pump();
    assert_eq!(*h.process.writes.lock().unwrap(), vec![b"R".to_vec()]);
    assert!(h.owner.workspace.lock().unwrap().reply.is_none());
    h.close();
}

/// A process that refuses for capacity is asked again; one that refuses for any
/// other reason has broken the contract, and the projection fails rather than
/// retrying a write the child will never accept.
#[test]
fn a_capacity_refusal_retries_but_any_other_write_error_fails_the_projection() {
    let h = Harness::standard();
    staged_reply(&h);
    h.process.reject.store(true, Ordering::Release);
    assert!(retry_soon(h.step()), "capacity refusal should retry");
    assert_eq!(h.owner.status().failure, None);
    assert!(h.owner.workspace.lock().unwrap().reply.is_some());

    h.process.reject.store(false, Ordering::Release);
    h.process.reject_io.store(true, Ordering::Release);
    assert!(matches!(h.step(), WorkSchedule::Dormant));
    assert_eq!(
        h.owner.status().failure,
        Some(ProjectionError::Process(ProcessError::Io))
    );
    assert!(
        h.owner.workspace.lock().unwrap().reply.is_none(),
        "a failed projection must not keep holding the reply buffer"
    );
    assert!(h.process.writes.lock().unwrap().is_empty());
}

/// A reply larger than the configured bound is zeroed before the projection
/// fails: it is engine output that was never admitted against any quota, so it
/// must not be left in memory or sent.
#[test]
fn an_oversized_generated_reply_is_zeroed_and_fails_the_projection() {
    let h = Harness::standard();
    let bound = options().terminal.reply_bytes;
    h.probe.reply_size.store(bound + 1, Ordering::Release);

    assert_eq!(h.owner.stage_output(b"?"), OutputAcceptance::Accepted);
    h.pump();
    assert_eq!(h.owner.status().failure, Some(ProjectionError::Capacity));
    assert!(h.owner.workspace.lock().unwrap().reply.is_none());
    assert!(h.process.writes.lock().unwrap().is_empty());
    assert_eq!(
        h.owner.status().processed.offset,
        0,
        "the chunk was not processed, so its position must not advance"
    );
}

/// A resize admitted before a child exists is put back, not answered: the
/// caller is still waiting and the size has not reached anything yet.
#[test]
fn a_resize_with_no_bound_process_is_requeued_and_the_caller_keeps_waiting() {
    let h = Harness::standard();
    h.owner.wiring.take_process();
    let target = TerminalSize::new(4, 3).unwrap();
    let mut resize = h.owner.resize(target).unwrap();

    assert!(matches!(h.step(), WorkSchedule::Dormant));
    assert!(matches!(poll(&mut resize), std::task::Poll::Pending));
    assert_eq!(h.owner.status().failure, None);
    assert_eq!(
        h.owner.status().control_generation,
        ControlGeneration::from_raw(0)
    );
    assert!(h.process.controls.lock().unwrap().is_empty());
    assert_eq!(h.owner.queue.queued(), 1, "the resize must still be queued");
    drop(resize);
}

/// A resize the OS accepted but the engine refused fails the projection: the
/// child's window and the model no longer agree, and nothing can reconcile them.
///
/// This is the opposite arm from the OS-failure test above, where the model was
/// never touched and the projection stays usable.
#[test]
fn a_model_resize_failure_after_an_accepted_os_resize_fails_the_projection() {
    let h = Harness::standard();
    h.probe.fail_resize.store(true, Ordering::Release);
    let target = TerminalSize::new(4, 3).unwrap();
    let mut resize = h.owner.resize(target).unwrap();
    h.pump();

    let outcome = result(&mut resize).unwrap();
    assert_eq!(outcome.os, Ok(()), "the child did resize");
    assert_eq!(
        outcome.model,
        Err(ProjectionError::Terminal(
            pty_runtime_domain::terminal::TerminalError::EngineFailure
        ))
    );
    assert_eq!(outcome.generation, ControlGeneration::from_raw(1));
    assert!(h.owner.status().failure.is_some());
    assert_eq!(
        h.owner.status().control_generation,
        ControlGeneration::from_raw(0),
        "a generation the model never applied must not be published"
    );
    assert!(
        !h.probe
            .trace
            .lock()
            .unwrap()
            .iter()
            .any(|t| matches!(t, Trace::Resize(_)))
    );
    drop(resize);
}

/// Output needs memory for a possible reply *before* it is fed, because the
/// engine may answer and that answer has nowhere else to go.
///
/// With every view byte pinned, the chunk goes back on the queue untouched
/// rather than being fed and then discovering there is nowhere to put the reply
/// — which would have mutated the model for a chunk the projection then has to
/// treat as unprocessed.
#[test]
fn output_with_no_reply_memory_is_requeued_unfed() {
    let h = Harness::new(
        options(),
        ProjectionLimits {
            view_bytes: options().view_reservation().unwrap(),
            ..ProjectionLimits::default()
        },
    );
    let mut pin = h.owner.view().unwrap();
    h.pump();
    let held = result(&mut pin).unwrap();

    assert_eq!(h.owner.stage_output(b"ab"), OutputAcceptance::Accepted);
    assert!(
        retry_soon(h.step()),
        "no reply memory should ask for a retry"
    );
    assert_eq!(h.owner.status().processed.offset, 0);
    assert_eq!(h.owner.queue.queued(), 1, "the chunk must still be queued");
    assert!(
        !h.probe
            .trace
            .lock()
            .unwrap()
            .iter()
            .any(|t| matches!(t, Trace::Feed(_))),
        "the chunk must not reach the engine without reply memory"
    );

    // Releasing the view lets the same chunk through unchanged.
    drop((held, pin));
    h.pump();
    assert_eq!(h.owner.status().processed.offset, 2);
    h.close();
}

/// Close must wait for a resize the child has already been told about.
///
/// The model is gone by then, so the caller cannot be told the resize took
/// effect; but the OS call is real and outstanding, and abandoning it would
/// leave the ticket unanswered and the child resized with nobody aware of it.
/// Cleanup stays dormant until the OS answers, then settles the caller with the
/// OS outcome and a model result of `Closed`.
#[test]
fn closing_waits_for_an_outstanding_resize_and_answers_it_as_closed() {
    let h = Harness::standard();
    h.process.hold_resize.store(true, Ordering::Release);
    let target = TerminalSize::new(4, 3).unwrap();
    let mut resize = h.owner.resize(target).unwrap();
    h.pump();
    assert_eq!(h.process.controls.lock().unwrap().len(), 1);

    let mut wait = h.owner.close().unwrap();
    h.pump();
    assert!(
        matches!(poll(&mut resize), std::task::Poll::Pending),
        "the caller must not be answered while the OS call is outstanding"
    );
    assert!(matches!(poll(&mut wait), std::task::Poll::Pending));
    assert_eq!(h.owner.status().residency, Residency::Closing);

    h.process.hold_resize.store(false, Ordering::Release);
    h.pump();
    let outcome = result(&mut resize).unwrap();
    assert_eq!(outcome.os, Ok(()), "the child really did resize");
    assert_eq!(outcome.model, Err(ProjectionError::Closed));
    assert_eq!(result(&mut wait), Ok(()));
    assert_eq!(h.owner.status().residency, Residency::Closed);
    drop((resize, wait));
    assert_budgets_released(&h);
}
