use super::{session::Session, signals::signal, watch::ExitWatch};
use pty_runtime_application::process::IProcessEvents;
use pty_runtime_domain::process::{ExitStatus, ProcessError};
use std::{
    fs::File,
    os::unix::process::ExitStatusExt,
    process::Child,
    sync::{Arc, atomic::Ordering},
    thread::JoinHandle,
    time::Instant,
};
pub(super) struct OwnedProcess {
    pub child: Child,
    pub _admission: super::spawner::Admission,
    pub host: File,
    pub watch: Option<ExitWatch>,
    pub session: Arc<Session>,
    pub events: Arc<dyn IProcessEvents>,
    pub reader: Option<JoinHandle<()>>,
    pub exit_at: Option<Instant>,
    pub cancel_at: Option<Instant>,
    pub killed: bool,
    pub supervision_lost: bool,
}
impl OwnedProcess {
    pub fn record_exit(&mut self, status: std::process::ExitStatus) {
        self.exit_at = Some(Instant::now());
        self.watch = None;
        self.session.finish_inputs(ProcessError::Closed);
        let status = match status.code() {
            Some(code) => ExitStatus::Code(code),
            None => ExitStatus::Signal(status.signal().unwrap_or(0)),
        };
        let _ =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.events.exited(status)));
    }
    pub fn reap(&mut self) {
        if self.exit_at.is_some() || self.supervision_lost {
            return;
        }
        match self.child.try_wait() {
            Ok(Some(status)) => self.record_exit(status),
            Ok(None) => {}
            Err(error) => {
                // Unexpected wait failure forfeits ownership. The host contract prohibits competing waiters; this is failure containment, not protection against races with them.
                self.supervision_lost = true;
                self.watch = None;
                self.session.finish_inputs(ProcessError::Internal);
                self.session.stop();
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    self.events.supervision_failed(super::error(error))
                }));
            }
        }
    }
    pub fn control(&mut self, now: Instant, shutting: bool, immediate: bool) {
        if self.exit_at.is_none() && !self.supervision_lost {
            if immediate || self.session.reader_failed.load(Ordering::Acquire) {
                signal(&self.child, &self.host, libc::SIGKILL);
                self.killed = true;
            }
            if !self.killed && (shutting || self.session.cancel.load(Ordering::Acquire)) {
                if self.cancel_at.is_none() {
                    signal(&self.child, &self.host, libc::SIGTERM);
                    self.cancel_at = Some(now);
                }
                if !self.killed
                    && self.cancel_at.is_some_and(|start| {
                        now.duration_since(start) >= self.session.limits.terminate_grace
                    })
                {
                    signal(&self.child, &self.host, libc::SIGKILL);
                    self.killed = true;
                }
            }
            let resize = self
                .session
                .queues
                .lock()
                .ok()
                .and_then(|mut q| q.resize.take());
            if let Some((size, reply)) = resize {
                let _ = reply.send(super::endpoints::resize(&self.host, size));
            }
        }
        if shutting
            || self
                .exit_at
                .is_some_and(|start| now.duration_since(start) >= self.session.limits.drain_timeout)
        {
            self.session.stop();
        }
    }
    pub fn deadline(&self) -> Option<Instant> {
        let cancel = self
            .cancel_at
            .filter(|_| !self.killed)
            .map(|start| start + self.session.limits.terminate_grace);
        let drain = self
            .exit_at
            .filter(|_| !self.session.stop_reader.load(Ordering::Acquire))
            .map(|start| start + self.session.limits.drain_timeout);
        let input = self
            .session
            .queues
            .lock()
            .ok()
            .and_then(|q| q.input.front().map(|input| input.deadline));
        [cancel, drain, input].into_iter().flatten().min()
    }
}
impl Drop for OwnedProcess {
    fn drop(&mut self) {
        self.session.stop();
        self.session.finish_inputs(ProcessError::Closed);
        if self.exit_at.is_none() && !self.supervision_lost {
            signal(&self.child, &self.host, libc::SIGKILL);
            let _ = self.child.wait();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        self.session.release_wakes();
    }
}
