//! A context published before projection admission fails must still settle its waiters.
use super::support::{Clock, Jobs, Protector, Scheduler, Signal, Store, options};
use crate::{
    diagnostics::RuntimeDiagnostics,
    process::{IProcessBackend, IProcessEvents, IProcessSession},
    projection::{ProjectionError, ProjectionServices},
    runtime::{
        ISessionRepository, Runtime, RuntimeError, RuntimeOptions, SessionContext, SessionOptions,
    },
    terminal::{ITerminal, ITerminalFactory},
};
use pty_runtime_domain::{
    SessionId, SessionLifetime,
    process::{CommandSpec, DrainOutcome, ProcessError, ProcessLimits},
    terminal::{
        TerminalCapabilities, TerminalCheckpoint, TerminalConfig, TerminalError, TerminalSize,
    },
};
use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    task::{Context, Poll, Wake, Waker},
    time::Duration,
};

struct RejectedFactory {
    entered: mpsc::SyncSender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}
impl ITerminalFactory for RejectedFactory {
    fn capabilities(&self) -> TerminalCapabilities {
        TerminalCapabilities {
            checkpoints: true,
            incremental_restore: false,
            mutation_during_restore: false,
            history_compression: false,
        }
    }
    fn compatibility(&self) -> &'static str {
        "rejected-admission-v1"
    }
    fn create(&self, _: TerminalConfig) -> Result<Box<dyn ITerminal>, TerminalError> {
        self.entered.send(()).unwrap();
        self.release
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        Err(TerminalError::EngineFailure)
    }
    fn restore(
        &self,
        _: TerminalCheckpoint,
        _: TerminalConfig,
    ) -> Result<Box<dyn ITerminal>, TerminalError> {
        panic!("failed initial admission must never restore")
    }
}
#[derive(Default)]
struct Backend(AtomicUsize);
impl IProcessBackend for Backend {
    fn spawn(
        &self,
        _: &CommandSpec,
        _: TerminalSize,
        _: SessionLifetime,
        _: ProcessLimits,
        _: Arc<dyn IProcessEvents>,
    ) -> Result<Arc<dyn IProcessSession>, ProcessError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Err(ProcessError::Internal)
    }
    fn shutdown(&self) {}
    fn shutdown_now(&self) {}
}
#[derive(Default)]
struct Registry(Mutex<HashMap<SessionId, Arc<SessionContext>>>);
impl ISessionRepository for Registry {
    fn register(
        &self,
        id: SessionId,
        context: Arc<SessionContext>,
        capacity: usize,
    ) -> Result<(), RuntimeError> {
        let mut map = self.0.lock().unwrap();
        if map.contains_key(&id) {
            return Err(RuntimeError::ExistingSession);
        }
        if map.len() >= capacity {
            return Err(RuntimeError::Capacity);
        }
        map.insert(id, context);
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
    fn remove_finished(&self, _: &SessionId, _: SessionLifetime) -> Result<(), RuntimeError> {
        Err(RuntimeError::NotFinished)
    }
    fn rollback_spawn(&self, id: &SessionId, lifetime: SessionLifetime) {
        let mut map = self.0.lock().unwrap();
        if map
            .get(id)
            .is_some_and(|context| context.lifetime() == lifetime)
        {
            map.remove(id);
        }
    }
}

#[derive(Default)]
struct WakeCount(AtomicUsize);
impl Wake for WakeCount {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn rejected_projection_admission_settles_published_waiter_and_releases_identity_and_model_slot() {
    let (entered, entering) = mpsc::sync_channel(1);
    let (release, released) = mpsc::sync_channel(1);
    let backend = Arc::new(Backend::default());
    let diagnostics = RuntimeDiagnostics::new();
    let runtime = Runtime::new(
        992,
        RuntimeOptions {
            max_sessions: 1,
            ..RuntimeOptions::default()
        },
        Arc::new(Registry::default()),
        backend.clone(),
    )
    .unwrap()
    .with_diagnostics(diagnostics.clone())
    .with_projection(ProjectionServices {
        terminal: Arc::new(RejectedFactory {
            entered,
            release: Mutex::new(released),
        }),
        clock: Arc::new(Clock::default()),
        scheduler: Arc::new(Scheduler),
        blocking: Arc::new(Jobs::default()),
        capacity: Arc::new(Signal::default()),
        store: Arc::new(Store::default()),
        protector: Arc::new(Protector::default()),
    })
    .unwrap();
    let id = SessionId::new("rejected-projection".into()).unwrap();
    let command = CommandSpec::new("/unused".into(), "/".into(), vec![]).unwrap();
    let error = RuntimeError::Projection(ProjectionError::Terminal(TerminalError::EngineFailure));
    let mut session_options = SessionOptions::projected(options().terminal);
    session_options.process.read_chunk = options().terminal.feed_bytes;
    let baseline = runtime.resources();
    let mut lifetimes = Vec::new();
    // A single session slot and the same ID force the second attempt to prove rollback.
    for _ in 0..2 {
        std::thread::scope(|scope| {
            let spawned =
                scope.spawn(|| runtime.spawn(id.clone(), &command, session_options.clone()));
            entering.recv_timeout(Duration::from_secs(5)).unwrap();
            let observer = runtime.lookup(&id).unwrap();
            lifetimes.push(observer.lifetime());
            let mut wait = observer.wait().unwrap();
            let wakes = Arc::new(WakeCount::default());
            let waker = Waker::from(wakes.clone());
            let mut cx = Context::from_waker(&waker);
            assert!(Pin::new(&mut wait).poll(&mut cx).is_pending());
            assert_eq!(diagnostics.aggregate().active_sessions, 1);
            assert_eq!(
                runtime
                    .resources()
                    .projection
                    .unwrap()
                    .native_reservations
                    .used,
                options().terminal.native_bytes
            );
            release.send(()).unwrap();
            assert!(matches!(spawned.join().unwrap(), Err(e) if e == error));
            assert!(
                wakes.0.load(Ordering::SeqCst) > 0,
                "admission failure did not wake the pending completion waiter"
            );
            assert!(matches!(
                runtime.lookup(&id),
                Err(RuntimeError::MissingSession)
            ));
            let Poll::Ready(Ok(completion)) = Pin::new(&mut wait).poll(&mut cx) else {
                panic!("published completion waiter was stranded by rejected projection admission");
            };
            assert_eq!(completion.status.admission_error, Some(error));
            assert_eq!(completion.status.supervision_error, None);
            assert_eq!(completion.status.exit, None);
            assert_eq!(completion.status.drain, Some(DrainOutcome::Eof));
            assert_eq!(backend.0.load(Ordering::SeqCst), 0);
            assert_eq!(diagnostics.aggregate().active_sessions, 0);
            drop((wait, observer));
            let resources = runtime.resources();
            assert_eq!(resources.observers, baseline.observers);
            assert_eq!(resources.replay_capacity, baseline.replay_capacity);
            assert_eq!(resources.input_bytes, baseline.input_bytes);
            assert_eq!(resources.input_slots, baseline.input_slots);
            let projection = resources.projection.unwrap();
            for budget in [
                projection.journal_bytes,
                projection.journal_slots,
                projection.transfer_observers,
                projection.staging_bytes,
                projection.staging_slots,
                projection.native_reservations,
                projection.checkpoint_buffers,
                projection.stored_bytes,
                projection.stored_slots,
                projection.views,
                projection.requests,
            ] {
                assert_eq!(
                    budget.used, 0,
                    "admission rollback retains a projection reservation"
                );
            }
        });
    }
    assert_ne!(
        lifetimes[0], lifetimes[1],
        "retry must have a fresh lifetime"
    );
    runtime.shutdown();
}

/// A factory whose compatibility identity violates the domain's bound.
///
/// After the identity rule was consolidated into `CompatibilityId`, the check in
/// `ProjectionCoordinator::create` became the only production guard against an
/// embedder-supplied factory: the two defence-in-depth copies inside the
/// protector and the checkpoint store were deleted as duplication.
struct BadIdentityFactory(&'static str);
impl ITerminalFactory for BadIdentityFactory {
    fn capabilities(&self) -> TerminalCapabilities {
        TerminalCapabilities {
            checkpoints: true,
            incremental_restore: false,
            mutation_during_restore: false,
            history_compression: false,
        }
    }
    fn compatibility(&self) -> &'static str {
        self.0
    }
    fn create(&self, _: TerminalConfig) -> Result<Box<dyn ITerminal>, TerminalError> {
        panic!("admission must be refused before any native state is created")
    }
    fn restore(
        &self,
        _: TerminalCheckpoint,
        _: TerminalConfig,
    ) -> Result<Box<dyn ITerminal>, TerminalError> {
        unreachable!()
    }
}

#[test]
fn factory_identity_outside_the_domain_bound_is_refused_before_native_creation() {
    const OVERSIZED: &str = match std::str::from_utf8(&[b'x'; 4097]) {
        Ok(value) => value,
        Err(_) => unreachable!(),
    };
    for identity in ["", OVERSIZED] {
        let services = ProjectionServices {
            terminal: Arc::new(BadIdentityFactory(identity)),
            clock: Arc::new(Clock::default()),
            scheduler: Arc::new(Scheduler),
            blocking: Arc::new(Jobs::default()),
            capacity: Arc::new(Signal::default()),
            store: Arc::new(Store::default()),
            protector: Arc::new(Protector::default()),
        };
        let budgets = crate::projection::ProjectionBudgets::new(Default::default()).unwrap();
        let outcome = crate::projection::ProjectionCoordinator::create(
            SessionLifetime::new(11, 1),
            options(),
            services,
            budgets,
            Arc::new(crate::runtime::quota::Quota::new(128)),
            Arc::new(crate::runtime::quota::Quota::new(8)),
        );
        // create() panics in the factory if it ever reaches native creation, so
        // reaching this assertion also proves the identity is checked first.
        assert!(matches!(
            outcome.err(),
            Some(ProjectionError::InvalidConfiguration)
        ));
    }
}
