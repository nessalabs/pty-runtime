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
