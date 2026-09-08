use super::{
    session::{Session, pair},
    spawn,
    spawner::{self, Admission, Message, Request},
    supervisor,
};
use pty_runtime_application::process::{IProcessBackend, IProcessEvents, IProcessSession};
use pty_runtime_domain::{
    SessionLifetime,
    process::{CommandSpec, ProcessError, ProcessLimits},
    terminal::TerminalSize,
};
use std::{
    io::Write,
    os::unix::net::UnixStream,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, SyncSender},
    },
    thread::JoinHandle,
};
pub(super) struct Shared {
    pub shutdown: AtomicBool,
    pub immediate: AtomicBool,
    pub spawn_stop: AtomicBool,
    pub active: AtomicUsize,
    pub max: usize,
    pub wake: Arc<UnixStream>,
}
/// Unix process owner. Owns one shared supervisor and one reader per admitted PTY.
/// Dropping the backend immediately kills/reaps owned children and joins all workers.
/// The host must neither reap managed children nor enable SIGCHLD auto-reaping for
/// this backend's entire lifetime. Construction rejects SIG_IGN/SA_NOCLDWAIT.
/// Cancellation currently targets the anchored original group, not arbitrary
/// foreground groups. Descendants leaving that group are outside this contract.
/// A blocked OS spawn/filesystem call cannot be interrupted portably: shutdown
/// reaps existing children independently, then joins that launch and cleans any
/// late child. Completion of that join has no wall-clock bound.
pub struct UnixProcessBackend {
    shared: Arc<Shared>,
    requests: SyncSender<Message>,
    worker: Mutex<Option<(JoinHandle<()>, JoinHandle<()>)>>,
}
impl UnixProcessBackend {
    /// Canonicalize launch roots and reserve a finite process admission limit.
    /// Roots constrain the initial directory only; they are not a filesystem sandbox.
    pub fn new(allowed_roots: Vec<PathBuf>, max_processes: usize) -> Result<Self, ProcessError> {
        Self::build(allowed_roots, max_processes, spawner::Options::default())
    }
    pub(super) fn build(
        allowed_roots: Vec<PathBuf>,
        max_processes: usize,
        options: spawner::Options,
    ) -> Result<Self, ProcessError> {
        super::signals::validate_host()?;
        if max_processes == 0 || max_processes > 65536 {
            return Err(ProcessError::Capacity);
        }
        let roots = spawn::roots(&allowed_roots)?;
        let (wake_tx, wake_rx) = pair()?;
        let shared = Arc::new(Shared {
            shutdown: AtomicBool::new(false),
            immediate: AtomicBool::new(false),
            spawn_stop: AtomicBool::new(false),
            active: AtomicUsize::new(0),
            max: max_processes,
            wake: Arc::new(wake_tx),
        });
        let (tx, rx) = mpsc::sync_channel(max_processes + 1);
        let (ready_tx, ready_rx) = mpsc::sync_channel(max_processes);
        let spawn_owner = shared.clone();
        let spawner = std::thread::Builder::new()
            .name("pty-spawner".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    spawner::run(spawn_owner.clone(), roots, rx, ready_tx, options)
                }));
                if result.is_err() {
                    spawn_owner.immediate.store(true, Ordering::Release);
                    spawn_owner.shutdown.store(true, Ordering::Release);
                    let _ = (&*spawn_owner.wake).write(&[1]);
                }
            })
            .map_err(super::error)?;
        let owner = shared.clone();
        let failure_tx = tx.clone();
        let worker = std::thread::Builder::new()
            .name("pty-supervisor".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    supervisor::run(owner.clone(), wake_rx, ready_rx)
                }));
                if result.is_err() {
                    owner.shutdown.store(true, Ordering::Release);
                    if !owner.spawn_stop.swap(true, Ordering::AcqRel) {
                        let _ = failure_tx.try_send(Message::Shutdown);
                    }
                }
            });
        let worker = match worker {
            Ok(worker) => worker,
            Err(error) => {
                shared.shutdown.store(true, Ordering::Release);
                let _ = tx.send(Message::Shutdown);
                let _ = spawner.join();
                return Err(super::error(error));
            }
        };
        Ok(Self {
            shared,
            requests: tx,
            worker: Mutex::new(Some((worker, spawner))),
        })
    }
}
impl IProcessBackend for UnixProcessBackend {
    fn spawn(
        &self,
        command: &CommandSpec,
        size: TerminalSize,
        _lifetime: SessionLifetime,
        limits: ProcessLimits,
        events: Arc<dyn IProcessEvents>,
    ) -> Result<Arc<dyn IProcessSession>, ProcessError> {
        limits.validate()?;
        if self.shared.shutdown.load(Ordering::Acquire) {
            return Err(ProcessError::Closed);
        }
        self.shared
            .active
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < self.shared.max).then_some(count + 1)
            })
            .map_err(|_| ProcessError::Capacity)?;
        let (reply, result) = mpsc::sync_channel::<Result<Arc<Session>, ProcessError>>(1);
        let request = Request {
            command: command.clone(),
            size,
            limits,
            events,
            reply,
            admission: Admission(self.shared.clone()),
        };
        if self
            .requests
            .try_send(Message::Launch(Box::new(request)))
            .is_err()
        {
            return Err(ProcessError::Closed);
        }
        let _ = (&*self.shared.wake).write(&[1]);
        result
            .recv()
            .map_err(|_| {
                if self.shared.shutdown.load(Ordering::Acquire) {
                    ProcessError::Closed
                } else {
                    ProcessError::Internal
                }
            })?
            .map(|session| session as Arc<dyn IProcessSession>)
    }
    fn shutdown(&self) {
        self.stop(false);
    }
    fn shutdown_now(&self) {
        self.stop(true);
    }
}
impl Drop for UnixProcessBackend {
    fn drop(&mut self) {
        self.shutdown_now();
    }
}

impl UnixProcessBackend {
    fn stop(&self, immediate: bool) {
        if immediate {
            self.shared.immediate.store(true, Ordering::Release);
        }
        self.shared.shutdown.store(true, Ordering::Release);
        if !self.shared.spawn_stop.swap(true, Ordering::AcqRel) {
            let _ = self.requests.try_send(Message::Shutdown);
        }
        let _ = (&*self.shared.wake).write(&[1]);
        if let Ok(mut workers) = self.worker.lock() {
            if let Some((supervisor, spawner)) = workers.take() {
                // Existing processes are reaped independently of a blocked launch.
                let _ = supervisor.join();
                let _ = spawner.join();
            }
        }
    }
}
