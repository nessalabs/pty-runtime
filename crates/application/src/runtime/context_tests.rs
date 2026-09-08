use super::{SessionContext, SessionOptions, context::Events, quota::Quota};
use crate::{diagnostics::RuntimeDiagnostics, process::IProcessEvents};
use pty_runtime_domain::{SessionLifetime, terminal::TerminalSize};
use std::sync::Arc;

#[test]
fn poisoned_context_drop_releases_owned_quotas_and_diagnostic_gauges() {
    let diagnostics = RuntimeDiagnostics::new();
    let observers = Arc::new(Quota::new(4));
    let replay = Arc::new(Quota::new(8));
    let mut options = SessionOptions::raw(TerminalSize::new(80, 24).unwrap());
    options.replay_bytes = 8;
    let context = Arc::new(
        SessionContext::new(
            SessionLifetime::new(88, 1),
            options,
            observers.clone(),
            replay.clone(),
            8,
            Arc::new(Quota::new(64)),
            Arc::new(Quota::new(4)),
        )
        .with_diagnostics(Some(diagnostics.clone())),
    );
    let events = Events(Arc::downgrade(&context));
    assert_eq!(
        events.output(b"synthetic marker"),
        crate::process::OutputAcceptance::Accepted
    );
    context.register_watcher().unwrap();
    assert_eq!(replay.usage().used, 8);
    assert_eq!(observers.usage().used, 1);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            context.record(|_| panic!("synthetic collaborator panic"));
        }))
        .is_err()
    );
    assert!(context.status().is_err());
    drop(context);
    assert_eq!(replay.usage().used, 0);
    assert_eq!(observers.usage().used, 0);
    assert_eq!(diagnostics.aggregate().active_sessions, 0);
    assert_eq!(diagnostics.aggregate().retained_replay_bytes, 0);
}

fn callback_context() -> (Arc<SessionContext>, Events, Arc<RuntimeDiagnostics>) {
    let diagnostics = RuntimeDiagnostics::new();
    let context = Arc::new(
        SessionContext::new(
            SessionLifetime::new(89, 1),
            SessionOptions::raw(TerminalSize::new(80, 24).unwrap()),
            Arc::new(Quota::new(4)),
            Arc::new(Quota::new(8)),
            8,
            Arc::new(Quota::new(64)),
            Arc::new(Quota::new(4)),
        )
        .with_diagnostics(Some(diagnostics.clone())),
    );
    let events = Events(Arc::downgrade(&context));
    (context, events, diagnostics)
}

#[test]
fn contradictory_callbacks_preserve_actual_facts_and_record_independent_failure() {
    use pty_runtime_domain::process::{DrainOutcome, ExitStatus, ProcessError};
    for contradict_exit in [false, true] {
        let (context, events, _) = callback_context();
        events.exited(ExitStatus::Code(7));
        events.drained(DrainOutcome::Eof);
        let original = context.status().unwrap();
        events.exited(ExitStatus::Code(7));
        events.drained(DrainOutcome::Eof);
        assert_eq!(context.status().unwrap(), original);
        if contradict_exit {
            events.exited(ExitStatus::Code(0));
        } else {
            events.drained(DrainOutcome::Failed(ProcessError::Io));
        }
        let status = context.status().unwrap();
        assert_eq!(status.exit, original.exit);
        assert_eq!(status.drain, original.drain);
        assert_eq!(status.supervision_error, Some(ProcessError::Internal));
        assert_eq!(status.admission_error, None);
        assert_eq!(status.completion().unwrap().status, status);
    }
}

#[test]
fn supervision_callbacks_count_reports_and_require_drain_without_fabricating_exit() {
    use crate::diagnostics::CounterKind;
    use pty_runtime_domain::process::{DrainOutcome, ProcessError};
    let (context, events, diagnostics) = callback_context();
    events.supervision_failed(ProcessError::PermissionDenied);
    assert!(context.status().unwrap().completion().is_none());
    assert_eq!(
        diagnostics.aggregate().count(CounterKind::FailedOperations),
        1
    );
    events.supervision_failed(ProcessError::Internal);
    assert_eq!(
        diagnostics.aggregate().count(CounterKind::FailedOperations),
        2
    );
    assert!(context.status().unwrap().completion().is_none());
    events.drained(DrainOutcome::Eof);
    let status = context.status().unwrap().completion().unwrap().status;
    assert_eq!(status.exit, None);
    assert_eq!(status.admission_error, None);
    assert_eq!(
        status.supervision_error,
        Some(ProcessError::PermissionDenied)
    );
    assert_eq!(status.drain, Some(DrainOutcome::Eof));
}
