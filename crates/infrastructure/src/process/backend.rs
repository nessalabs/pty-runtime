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
    pub image: super::image::HelperImage,
    pub shutdown: AtomicBool,
    pub immediate: AtomicBool,
    pub spawn_stop: AtomicBool,
    pub active: AtomicUsize,
    pub max: usize,
    pub wake: Arc<UnixStream>,
}
/// Unix process owner with a shared host supervisor/spawner and dedicated PTY readers.
/// Each admitted PTY additionally owns two fresh helper processes and bounded
/// temporary group anchors, including a cleanup successor when both helper groups
/// contain descendants. These per-session helper event loops have explicit OS cost.
/// Dropping immediately requests cleanup, observes helper termination and joins workers.
/// Construction also owns a short-lived child that stages the helper image, so
/// caller forks never inherit its executable writer. It uses raw file operations
/// with signals blocked. Hard host precondition: call `new` only from a
/// single-threaded window (before other threads exist, or with no concurrent
/// locks). Platform atfork handlers can block indefinitely if another thread
/// holds a lock across this fork. Filesystem I/O and waiting for this child have
/// no fixed wall-clock bound.
/// The host must neither reap managed children nor enable SIGCHLD auto-reaping for
/// this backend's entire lifetime. Construction rejects SIG_IGN/SA_NOCLDWAIT.
/// Cancellation uses verified member anchors for root/foreground groups; a live
/// helper retains the owned session through single-helper failure. Discovery
/// errors retain cleanup ownership and can delay shutdown until the OS recovers.
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
    /// Verify a bundled helper against this build's exact target image and stage
    /// a private executable copy. Signatures embedded in those bytes are preserved.
    /// A signed package supplies the same image at build time through
    /// `PTY_RUNTIME_GUARDIAN_IMAGE`; version/architecture mismatches are rejected.
    pub fn with_bundled_guardian(
        allowed_roots: Vec<PathBuf>,
        max_processes: usize,
        image: PathBuf,
    ) -> Result<Self, ProcessError> {
        Self::build(
            allowed_roots,
            max_processes,
            spawner::Options {
                bundled: Some(image),
                #[cfg(test)]
                hook: None,
                #[cfg(test)]
                after_launch: None,
            },
        )
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
        let image = super::image::HelperImage::new(options.bundled.as_deref())?;
        let (wake_tx, wake_rx) = pair()?;
        let shared = Arc::new(Shared {
            image,
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
