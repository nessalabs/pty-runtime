//! Session delivery, input admission and bounded diagnostics.
use super::{
    AttachPosition, RuntimeError, Session, SessionContext, SessionOptions,
    context::SessionEventSink, quota::Quota,
};
use crate::{
    diagnostics::{CounterKind, RuntimeDiagnostics},
    process::{IProcessEvents, OutputAcceptance},
};
use pty_runtime_domain::{
    SessionLifetime,
    process::{DrainOutcome, ExitStatus},
    terminal::TerminalSize,
};
use std::sync::Arc;

fn fixture(local_observers: usize) -> (Session, SessionEventSink, Arc<RuntimeDiagnostics>) {
    let mut options = SessionOptions::raw(TerminalSize::new(80, 24).unwrap());
    options.max_observers = local_observers;
    let diagnostics = RuntimeDiagnostics::new();
    let context = Arc::new(
        SessionContext::new(
            SessionLifetime::new(91, 1),
            options,
            Arc::new(Quota::new(8)),
            Arc::new(Quota::new(128)),
            16,
            Arc::new(Quota::new(64)),
            Arc::new(Quota::new(2)),
        )
        .with_diagnostics(Some(diagnostics.clone())),
    );
    (
        Session {
            context: context.clone(),
        },
        SessionEventSink(Arc::downgrade(&context)),
        diagnostics,
    )
}
#[test]
fn saturated_input_is_counted_and_does_not_prevent_cancellation_before_binding() {
    let (session, _, diagnostics) = fixture(2);
    assert!(matches!(
        session.write(&[0; 65]),
        Err(RuntimeError::Capacity)
    ));
    assert_eq!(session.context.input_slots.usage().used, 0);
    assert_eq!(
        diagnostics.aggregate().count(CounterKind::InputSaturation),
        1
    );
    assert_eq!(session.cancel(), Ok(()));
    assert!(session.status().unwrap().cancellation_requested);
    assert_eq!(session.process_id(), Err(RuntimeError::Closed));
}

#[test]
fn callbacks_after_last_handle_drop_stop_output_without_resurrecting_a_session() {
    let (session, events, diagnostics) = fixture(2);
    drop(session);
    assert_eq!(events.output(b"late"), OutputAcceptance::Closed);
    assert!(events.diagnostics().is_none());
    events.wait_for_capacity(std::time::Instant::now());
    events.exited(ExitStatus::Code(0));
    events.drained(DrainOutcome::Eof);
    events.supervision_failed(pty_runtime_domain::process::ProcessError::Io);
    assert_eq!(diagnostics.aggregate().active_sessions, 0);
}

#[test]
fn immediate_completion_reads_are_stable_and_cancellation_refuses_finished_sessions() {
    let (session, events, _) = fixture(2);
    let mut attachment = session.attach(AttachPosition::Tail).unwrap();
    assert_eq!(attachment.try_next().unwrap(), None);
    events.exited(ExitStatus::Code(4));
    events.drained(DrainOutcome::Eof);
    let cursor = attachment.cursor();
    let first = attachment.try_next().unwrap().unwrap();
    assert!(matches!(first, super::OutputEvent::Complete(_)));
    assert_eq!(attachment.try_next().unwrap(), Some(first));
    assert_eq!(attachment.cursor(), cursor);
    assert_eq!(session.cancel(), Err(RuntimeError::Closed));
}

#[test]
fn fixed_diagnostics_storage_remains_bounded_independently_of_recorded_samples() {
    let diagnostics = RuntimeDiagnostics::new();
    let bytes = RuntimeDiagnostics::storage_bytes();
    assert!(bytes > 0 && bytes <= 80 * 1024);
    for _ in 0..1024 {
        diagnostics.count(CounterKind::InputSaturation, 1);
    }
    assert_eq!(RuntimeDiagnostics::storage_bytes(), bytes);
    assert_eq!(
        diagnostics.aggregate().count(CounterKind::InputSaturation),
        1024
    );
}
