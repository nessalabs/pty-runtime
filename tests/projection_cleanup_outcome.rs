//! Independent runtime close-outcome regression; fake providers prove wiring only.
mod terminal {
    pub use pty_runtime_application::terminal::{ITerminal, ITerminalFactory};
}
mod checkpoint {
    pub use pty_runtime_application::checkpoint::{ICheckpointProtector, ICheckpointStore};
}
mod process {
    pub use pty_runtime_application::process::*;
}
#[path = "../crates/application/src/projection/tests/terminal.rs"]
mod native_fixture;
#[path = "../crates/application/src/projection/tests/process.rs"]
mod process_fixture;
#[path = "../crates/application/src/projection/tests/providers.rs"]
mod provider_fixture;
use pty_runtime::{ports::*, *};
use pty_runtime_infrastructure::{registry::MemorySessionRepository, scheduling::*};
use std::{
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};
struct CompletedBackend;
impl IProcessBackend for CompletedBackend {
    fn spawn(
        &self,
        _: &CommandSpec,
        _: TerminalSize,
        _: SessionLifetime,
        _: ProcessLimits,
        events: Arc<dyn IProcessEvents>,
    ) -> Result<Arc<dyn IProcessSession>, ProcessError> {
        events.exited(ExitStatus::Code(0));
        events.drained(DrainOutcome::Eof);
        Ok(Arc::new(process_fixture::Process::default()))
    }
    fn shutdown(&self) {}
    fn shutdown_now(&self) {}
}
#[test]
fn forget_preserves_delete_and_uncertain_commit_failures_including_repeated_calls() {
    for uncertain in [false, true] {
        let store = Arc::new(provider_fixture::Store::default());
        store.invalid_reference.store(uncertain, Ordering::Release);
        store.fail_delete.store(!uncertain, Ordering::Release);
        let runtime = Runtime::with_projection_adapters(
            RuntimeOptions::default(),
            Arc::new(MemorySessionRepository::default()),
            Arc::new(CompletedBackend),
            ProjectionServices {
                terminal: Arc::new(native_fixture::Factory(Arc::new(
                    native_fixture::Probe::default(),
                ))),
                clock: Arc::new(MonotonicSystemClock::default()),
                scheduler: Arc::new(StdWorkScheduler::with_workers(2, 1).unwrap()),
                blocking: Arc::new(BoundedBlockingExecutor::new(2, 1).unwrap()),
                capacity: Arc::new(CondvarCapacitySignal::default()),
                store: store.clone(),
                protector: Arc::new(provider_fixture::Protector::default()),
            },
        )
        .unwrap();
        let mut options = SessionOptions::projected(pty_runtime::terminal::TerminalConfig {
            size: TerminalSize::new(2, 1).unwrap(),
            history_bytes: 128,
            continuation_bytes: 128,
            reply_bytes: 32,
            checkpoint_bytes: 1024,
            native_bytes: 4096,
            view_bytes: 1024,
            feed_bytes: 8192,
        });
        let projection = options.projection.as_mut().unwrap();
        projection.park_after = Duration::from_millis(1);
        projection.retry_after = Duration::from_millis(1);
        projection.max_park_attempts = 1;
        let id = SessionId::new("cleanup".into()).unwrap();
        let session = runtime
            .spawn(
                id.clone(),
                &CommandSpec::new("/unused".into(), "/".into(), vec![]).unwrap(),
                options,
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let status = session.projection_status().unwrap().unwrap();
            if (!uncertain && status.residency == Residency::Parked)
                || (uncertain && status.parking_failure.is_some())
            {
                break;
            }
            assert!(Instant::now() < deadline, "parking did not settle");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(matches!(
            runtime.forget(&id),
            Err(RuntimeError::Projection(_))
        ));
        assert!(matches!(
            runtime.forget(&id),
            Err(RuntimeError::Projection(_))
        ));
        assert!(runtime.lookup(&id).is_ok());
        assert_eq!(
            session.projection_status().unwrap().unwrap().residency,
            Residency::Closed
        );
        runtime.shutdown();
    }
}

#[test]
fn domain_session_validation_rejects_incompatible_projection_admission() {
    let runtime = RuntimeOptions::default();
    let valid = SessionOptions::projected(pty_runtime::terminal::TerminalConfig {
        size: TerminalSize::new(2, 1).unwrap(),
        history_bytes: 128,
        continuation_bytes: 128,
        reply_bytes: 32,
        checkpoint_bytes: 1024,
        native_bytes: 4096,
        view_bytes: 1024,
        feed_bytes: 8192,
    });
    assert!(valid.validate(&runtime).is_ok());
    for case in 0..7 {
        let mut session = valid.clone();
        let mut limits = runtime.clone();
        match case {
            0 => session.size = TerminalSize::new(3, 1).unwrap(),
            1 => session.projection.as_mut().unwrap().terminal.feed_bytes = 2048,
            2 => session.projection.as_mut().unwrap().staging_bytes = 4096,
            3 => limits.projection.staging_bytes = 2048,
            4 => limits.projection.view_bytes = 16,
            5 => limits.input_bytes = 16,
            _ => session.process.input_chunk = 16,
        }
        assert!(session.validate(&limits).is_err(), "case {case} accepted");
    }
    let mut no_observers = valid;
    no_observers.max_observers = 0;
    assert_eq!(no_observers.validate(&runtime), Err(RuntimeError::Capacity));
}
