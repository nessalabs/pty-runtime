//! Public runtime ownership with external repository/process port doubles.
use super::*;
use crate::process::{
    IInputReservation, IProcessBackend, IProcessEvents, IProcessSession, ProcessOperation,
};
use pty_runtime_domain::{
    SessionId, SessionLifetime,
    process::{CommandSpec, DrainOutcome, ExitStatus, ProcessError, ProcessLimits, WriteOutcome},
    terminal::{TerminalConfig, TerminalSize},
};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Default)]
struct Repository(Mutex<HashMap<SessionId, Arc<SessionContext>>>);
impl ISessionRepository for Repository {
    fn register(
        &self,
        id: SessionId,
        context: Arc<SessionContext>,
        limit: usize,
    ) -> Result<(), RuntimeError> {
        let mut entries = self.0.lock().unwrap();
        if entries.contains_key(&id) {
            return Err(RuntimeError::ExistingSession);
        }
        if entries.len() == limit {
            return Err(RuntimeError::Capacity);
        }
        entries.insert(id, context);
        Ok(())
    }
    fn lookup(&self, id: &SessionId) -> Result<Arc<SessionContext>, RuntimeError> {
        self.0
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or(RuntimeError::MissingSession)
    }
    fn remove_finished(
        &self,
        id: &SessionId,
        lifetime: SessionLifetime,
    ) -> Result<(), RuntimeError> {
        let mut entries = self.0.lock().unwrap();
        let context = entries.get(id).ok_or(RuntimeError::MissingSession)?;
        if context.lifetime() != lifetime || context.completion()?.is_none() {
            return Err(RuntimeError::NotFinished);
        }
        entries.remove(id);
        Ok(())
    }
    fn rollback_spawn(&self, id: &SessionId, lifetime: SessionLifetime) {
        let mut entries = self.0.lock().unwrap();
        if entries
            .get(id)
            .is_some_and(|context| context.lifetime() == lifetime)
        {
            entries.remove(id);
        }
    }
}
#[derive(Default)]
struct Process {
    refuse: AtomicBool,
    cancelled: AtomicBool,
}
impl IProcessSession for Process {
    fn process_id(&self) -> u32 {
        42
    }
    fn write_reserved(
        &self,
        bytes: &[u8],
        reservation: Option<Box<dyn IInputReservation>>,
    ) -> Result<ProcessOperation<WriteOutcome>, ProcessError> {
        drop(reservation);
        if self.refuse.load(Ordering::Acquire) {
            return Err(ProcessError::Io);
        }
        let count = bytes.len();
        Ok(Box::pin(async move {
            WriteOutcome {
                written: count,
                error: None,
            }
        }))
    }
    fn request_cancel(&self) -> Result<(), ProcessError> {
        self.cancelled.store(true, Ordering::Release);
        if self.refuse.load(Ordering::Acquire) {
            Err(ProcessError::PermissionDenied)
        } else {
            Ok(())
        }
    }
    fn resize(
        &self,
        _: TerminalSize,
    ) -> Result<ProcessOperation<Result<(), ProcessError>>, ProcessError> {
        Err(ProcessError::Io)
    }
}
#[derive(Default)]
struct Backend {
    process: Arc<Process>,
    events: Mutex<Option<Arc<dyn IProcessEvents>>>,
}
impl IProcessBackend for Backend {
    fn spawn(
        &self,
        _: &CommandSpec,
        _: TerminalSize,
        _: SessionLifetime,
        _: ProcessLimits,
        events: Arc<dyn IProcessEvents>,
    ) -> Result<Arc<dyn IProcessSession>, ProcessError> {
        *self.events.lock().unwrap() = Some(events);
        Ok(self.process.clone())
    }
    fn shutdown(&self) {}
    fn shutdown_now(&self) {}
}
fn command() -> CommandSpec {
    CommandSpec::new("/unused".into(), "/".into(), vec![]).unwrap()
}
fn id() -> SessionId {
    SessionId::new("coverage-boundary".into()).unwrap()
}
fn settings() -> SessionOptions {
    SessionOptions::raw(TerminalSize::new(80, 24).unwrap())
}
fn fixture() -> (Runtime, Arc<Backend>) {
    let backend = Arc::new(Backend::default());
    let owner = Runtime::new(
        93,
        RuntimeOptions::default(),
        Arc::new(Repository::default()),
        backend.clone(),
    )
    .unwrap();
    (owner, backend)
}

#[test]
fn forgetting_requires_completion_and_new_identity_does_not_retarget_old_handles() {
    let (owner, backend) = fixture();
    assert_eq!(owner.forget(&id()), Err(RuntimeError::MissingSession));
    let first = owner.spawn(id(), &command(), settings()).unwrap();
    assert_eq!(first.process_id(), Ok(42));
    assert_eq!(owner.forget(&id()), Err(RuntimeError::NotFinished));
    let events = backend.events.lock().unwrap().clone().unwrap();
    events.exited(ExitStatus::Code(7));
    assert_eq!(owner.forget(&id()), Err(RuntimeError::NotFinished));
    events.drained(DrainOutcome::Eof);
    assert_eq!(owner.forget(&id()), Ok(()));
    assert!(matches!(
        owner.lookup(&id()),
        Err(RuntimeError::MissingSession)
    ));
    let second = owner.spawn(id(), &command(), settings()).unwrap();
    assert_ne!(first.lifetime(), second.lifetime());
    assert_eq!(first.status().unwrap().exit, Some(ExitStatus::Code(7)));
    assert_eq!(second.status().unwrap().exit, None);
}

#[test]
fn a_raw_runtime_refuses_projected_spawn_and_projection_operations_explicitly() {
    let (owner, _) = fixture();
    let unsupported =
        RuntimeError::Projection(pty_runtime_domain::projection::ProjectionError::Terminal(
            pty_runtime_domain::terminal::TerminalError::Unsupported,
        ));
    let projection =
        SessionOptions::projected(TerminalConfig::new(TerminalSize::new(80, 24).unwrap()));
    assert!(
        matches!(owner.spawn(id(), &command(), projection), Err(error) if error == unsupported)
    );
    let session = owner.spawn(id(), &command(), settings()).unwrap();
    assert!(session.program_status().unwrap().is_none());
    assert!(session.projection_status().unwrap().is_none());
    assert!(matches!(session.projected_view(), Err(error) if error == unsupported));
    assert!(matches!(session.terminal_checkpoint(), Err(error) if error == unsupported));
    assert!(matches!(session.begin_transfer(), Err(error) if error == unsupported));
    assert!(
        matches!(session.resize_projected(TerminalSize::new(3,2).unwrap()), Err(error) if error == unsupported)
    );
}

#[test]
fn shutdown_refuses_new_spawns_without_reusing_a_reserved_identity() {
    let (owner, _) = fixture();
    owner.shutdown();
    assert!(matches!(
        owner.spawn(id(), &command(), settings()),
        Err(RuntimeError::Closed)
    ));
    assert!(matches!(
        owner.lookup(&id()),
        Err(RuntimeError::MissingSession)
    ));
    assert_eq!(owner.resources().observers.used, 0);
}

#[test]
fn adapter_refusals_release_write_admission_and_keep_cancel_intent_visible() {
    let (owner, backend) = fixture();
    let session = owner.spawn(id(), &command(), settings()).unwrap();
    backend.process.refuse.store(true, Ordering::Release);
    assert!(matches!(
        session.write(b"private"),
        Err(RuntimeError::Process(ProcessError::Io))
    ));
    assert_eq!(owner.resources().input_bytes.used, 0);
    assert_eq!(owner.resources().input_slots.used, 0);
    assert_eq!(
        session.cancel(),
        Err(RuntimeError::Process(ProcessError::PermissionDenied))
    );
    assert!(session.status().unwrap().cancellation_requested);
    assert!(matches!(
        session.resize(TerminalSize::new(2, 2).unwrap()),
        Err(RuntimeError::Process(ProcessError::Io))
    ));
}

#[test]
fn observer_capacity_failure_returns_the_shared_reservation() {
    let (owner, _) = fixture();
    let mut options = settings();
    options.max_observers = 1;
    let session = owner.spawn(id(), &command(), options).unwrap();
    let first = session.attach(AttachPosition::Tail).unwrap();
    assert!(matches!(session.wait(), Err(RuntimeError::Capacity)));
    assert_eq!(owner.resources().observers.used, 1);
    drop(first);
    assert_eq!(owner.resources().observers.used, 0);
    let replacement = session.wait().unwrap();
    assert_eq!(owner.resources().observers.used, 1);
    drop(replacement);
    assert_eq!(owner.resources().observers.used, 0);
}

struct BlockedSpawnBackend {
    process: Arc<Process>,
    entered: std::sync::mpsc::Sender<()>,
    release: Mutex<std::sync::mpsc::Receiver<()>>,
    shutdown_entered: std::sync::mpsc::Sender<()>,
}
impl IProcessBackend for BlockedSpawnBackend {
    fn spawn(
        &self,
        _: &CommandSpec,
        _: TerminalSize,
        _: SessionLifetime,
        _: ProcessLimits,
        _: Arc<dyn IProcessEvents>,
    ) -> Result<Arc<dyn IProcessSession>, ProcessError> {
        self.entered.send(()).unwrap();
        self.release
            .lock()
            .unwrap()
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        Ok(self.process.clone())
    }
    fn shutdown(&self) {
        self.shutdown_entered.send(()).unwrap();
    }
    fn shutdown_now(&self) {}
}

#[test]
fn shutdown_refuses_new_work_and_cancels_an_already_admitted_spawn() {
    let (entered, entry) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel();
    let (shutdown_entered, shutdown_entry) = std::sync::mpsc::channel();
    let process = Arc::new(Process::default());
    let owner = Runtime::new(
        94,
        RuntimeOptions::default(),
        Arc::new(Repository::default()),
        Arc::new(BlockedSpawnBackend {
            process: process.clone(),
            entered,
            release: Mutex::new(released),
            shutdown_entered,
        }),
    )
    .unwrap();
    std::thread::scope(|scope| {
        let spawn = scope.spawn(|| owner.spawn(id(), &command(), settings()));
        entry
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        let shutdown = scope.spawn(|| owner.shutdown());
        shutdown_entry
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(matches!(
            owner.spawn(
                SessionId::new("after-shutdown".into()).unwrap(),
                &command(),
                settings()
            ),
            Err(RuntimeError::Closed)
        ));
        release.send(()).unwrap();
        let session = spawn.join().unwrap().unwrap();
        shutdown.join().unwrap();
        assert_eq!(session.process_id(), Ok(42));
        assert!(process.cancelled.load(Ordering::Acquire));
    });
}
