use super::{backend::Shared, guardian::Guardian, session::Session, spawn};
use pty_runtime_application::process::IProcessEvents;
use pty_runtime_domain::{
    process::{CommandSpec, ProcessError, ProcessLimits},
    terminal::TerminalSize,
};
use std::{
    fs::File,
    io::Write,
    path::PathBuf,
    sync::{
        Arc,
        atomic::Ordering,
        mpsc::{Receiver, SyncSender},
    },
};
/// This reservation moves across queues and drops only after child cleanup.
pub(super) struct Admission(pub Arc<Shared>);
impl Drop for Admission {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::AcqRel);
    }
}
pub(super) struct Request {
    pub command: CommandSpec,
    pub size: TerminalSize,
    pub limits: ProcessLimits,
    pub events: Arc<dyn IProcessEvents>,
    pub reply: SyncSender<Result<Arc<Session>, ProcessError>>,
    pub admission: Admission,
}
pub(super) enum Message {
    Launch(Box<Request>),
    Shutdown,
}
pub(super) struct PendingChild(pub Option<(Guardian, File)>);
impl PendingChild {
    pub fn take(&mut self) -> Option<(Guardian, File)> {
        self.0.take()
    }
}
impl Drop for PendingChild {
    fn drop(&mut self) {
        if let Some((child, host)) = &mut self.0 {
            child.cleanup(host);
        }
    }
}
pub(super) struct Prepared {
    pub child: PendingChild,
    pub request: Request,
}
#[derive(Default)]
pub(super) struct Options {
    pub bundled: Option<PathBuf>,
    #[cfg(test)]
    pub hook: Option<Arc<dyn Fn() -> Result<(), ProcessError> + Send + Sync>>,
    #[cfg(test)]
    pub after_launch: Option<Arc<dyn Fn(u32) + Send + Sync>>,
}
pub(super) fn run(
    shared: Arc<Shared>,
    roots: Vec<spawn::LaunchRoot>,
    requests: Receiver<Message>,
    ready: SyncSender<Prepared>,
    options: Options,
) {
    #[cfg(not(test))]
    let _ = options;
    while let Ok(message) = requests.recv() {
        let Message::Launch(request) = message else {
            break;
        };
        let request = *request;
        if shared.shutdown.load(Ordering::Acquire) {
            reject(request, ProcessError::Closed);
            continue;
        }
        #[cfg(test)]
        if let Some(hook) = &options.hook {
            if let Err(error) = hook() {
                reject(request, error);
                continue;
            }
        }
        match spawn::launch(
            &request.command,
            request.size,
            &roots,
            &shared.image,
            // The guardian's own deadline, not the owner's: it escalates as a
            // backstop for an owner that never asks, so it must wait longer.
            request.limits.guardian_grace(),
        ) {
            Ok(child) => {
                #[cfg(test)]
                if let Some(hook) = &options.after_launch {
                    hook(child.0.id());
                }
                let prepared = Prepared {
                    request,
                    child: PendingChild(Some(child)),
                };
                if shared.shutdown.load(Ordering::Acquire) {
                    let reply = prepared.request.reply.clone();
                    drop(prepared);
                    let _ = reply.send(Err(ProcessError::Closed));
                    continue;
                }
                if let Err(failed) = ready.try_send(prepared) {
                    let failed = match failed {
                        std::sync::mpsc::TrySendError::Full(value)
                        | std::sync::mpsc::TrySendError::Disconnected(value) => value,
                    };
                    let reply = failed.request.reply.clone();
                    drop(failed);
                    let _ = reply.send(Err(ProcessError::Closed));
                }
                let _ = (&*shared.wake).write(&[1]);
            }
            Err(error) => reject(request, error),
        }
    }
    // Shutdown can overtake already-admitted work; release all unsent commands.
    while let Ok(message) = requests.try_recv() {
        if let Message::Launch(request) = message {
            reject(*request, ProcessError::Closed);
        }
    }
}
fn reject(request: Request, error: ProcessError) {
    let reply = request.reply.clone();
    drop(request);
    let _ = reply.send(Err(error));
}
