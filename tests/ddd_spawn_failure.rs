//! Independent adversarial proof for observers obtained during a failed spawn.
use pty_runtime_application::{
    process::{IProcessBackend, IProcessEvents, IProcessSession},
    runtime::{Runtime, RuntimeOptions, SessionOptions},
};
use pty_runtime_domain::{
    SessionId, SessionLifetime,
    process::{CommandSpec, ProcessError, ProcessLimits},
    terminal::TerminalSize,
};
use pty_runtime_infrastructure::registry::MemorySessionRepository;
use std::sync::{Arc, Barrier};

struct FailingBackend {
    entered: Barrier,
    release: Barrier,
}
impl IProcessBackend for FailingBackend {
    fn spawn(
        &self,
        _: &CommandSpec,
        _: TerminalSize,
        _: SessionLifetime,
        _: ProcessLimits,
        _: Arc<dyn IProcessEvents>,
    ) -> Result<Arc<dyn IProcessSession>, ProcessError> {
        self.entered.wait();
        self.release.wait();
        Err(ProcessError::NotFound)
    }
    fn shutdown(&self) {}
    fn shutdown_now(&self) {}
}

#[test]
fn lookup_handle_obtained_during_failed_spawn_reaches_terminal_state() {
    let backend = Arc::new(FailingBackend {
        entered: Barrier::new(2),
        release: Barrier::new(2),
    });
    let runtime = Runtime::new(
        991,
        RuntimeOptions::default(),
        Arc::new(MemorySessionRepository::default()),
        backend.clone(),
    )
    .unwrap();
    let id = SessionId::new("failed-spawn-observer".into()).unwrap();
    let command = CommandSpec::new("/missing-executable".into(), "/".into(), vec![]).unwrap();
    std::thread::scope(|scope| {
        let spawning = scope.spawn(|| {
            runtime.spawn(
                id.clone(),
                &command,
                SessionOptions::raw(TerminalSize::new(80, 24).unwrap()),
            )
        });
        backend.entered.wait();
        let earlier_handle = runtime.lookup(&id).unwrap();
        backend.release.wait();
        assert!(spawning.join().unwrap().is_err());
        assert!(runtime.lookup(&id).is_err());
        assert!(
            earlier_handle.status().unwrap().completion().is_some(),
            "a published starting context must terminate when spawn fails; existing waits cannot remain pending forever"
        );
    });
}
