use super::{guardian::Guardian, protocol::Kind, session::Session};
use pty_runtime_application::process::IProcessEvents;
use pty_runtime_domain::process::{ExitStatus, ProcessError};
use std::{
    fs::File,
    os::unix::process::ExitStatusExt,
    sync::{Arc, atomic::Ordering},
    thread::JoinHandle,
    time::{Duration, Instant},
};

pub(super) struct OwnedProcess {
    pub guardian: Guardian,
    pub _admission: super::spawner::Admission,
    pub host: File,
    pub session: Arc<Session>,
    pub events: Arc<dyn IProcessEvents>,
    pub reader: Option<JoinHandle<()>>,
    pub exit_at: Option<Instant>,
    pub cancel_at: Option<Instant>,
    pub killed: bool,
    pub supervision_lost: bool,
    discard_at: Option<Instant>,
}
impl OwnedProcess {
    pub fn new(
        guardian: Guardian,
        admission: super::spawner::Admission,
        host: File,
        session: Arc<Session>,
        events: Arc<dyn IProcessEvents>,
        reader: JoinHandle<()>,
    ) -> Self {
        Self {
            guardian,
            _admission: admission,
            host,
            session,
            events,
            reader: Some(reader),
            exit_at: None,
            cancel_at: None,
            killed: false,
            supervision_lost: false,
            discard_at: None,
        }
    }
    pub fn record_exit(&mut self, status: std::process::ExitStatus) {
        if self.exit_at.is_some() {
            return;
        }
        let status = if let Some(code) = status.code() {
            ExitStatus::Code(code)
        } else if let Some(signal) = status.signal() {
            ExitStatus::Signal(signal)
        } else {
            self.report_failure();
            return;
        };
        self.exit_at = Some(Instant::now());
        self.session.finish_inputs(ProcessError::Closed);
        let _ =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.events.exited(status)));
    }
    fn report_failure(&mut self) {
        if self.supervision_lost {
            return;
        }
        self.supervision_lost = true;
        self.session.finish_inputs(ProcessError::Internal);
        self.session.stop();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.events.supervision_failed(ProcessError::Internal)
        }));
    }
    pub fn reap(&mut self) {
        self.guardian.service();
        if self.guardian.take_escalation_applied() {
            if let Some(diagnostics) = &self.session.diagnostics {
                diagnostics.count(pty_runtime_application::diagnostics::CounterKind::AcknowledgedWorkloadEscalations, 1);
            }
        }
        if self.guardian.take_term_applied() {
            let timing = self
                .session
                .cancel_timing
                .lock()
                .ok()
                .and_then(|mut pending| pending.take());
            if let Some(timing) = timing {
                timing.finish(true);
            }
        }
        if let Some(status) = self.guardian.take_exit() {
            self.record_exit(status);
        }
        if self.guardian.take_fault() {
            self.report_failure();
        }
    }
    pub fn control(&mut self, now: Instant, shutting: bool, immediate: bool) {
        if immediate || self.supervision_lost || self.session.reader_failed.load(Ordering::Acquire)
        {
            self.guardian.request(Kind::Abort);
            self.killed = true;
        }
        if self.exit_at.is_none() && !self.supervision_lost {
            if !self.killed && (shutting || self.session.cancel.load(Ordering::Acquire)) {
                if self.cancel_at.is_none() {
                    self.guardian.request(Kind::Terminate);
                    self.cancel_at = Some(now);
                }
                if self.cancel_at.is_some_and(|start| {
                    now.duration_since(start) >= self.session.limits.terminate_grace
                }) {
                    self.guardian.request(Kind::Kill);
                    if let Some(diagnostics) = &self.session.diagnostics {
                        diagnostics.count(pty_runtime_application::diagnostics::CounterKind::CancellationEscalations, 1);
                    }
                    self.killed = true;
                }
            }
            let resize = self
                .session
                .queues
                .lock()
                .ok()
                .and_then(|mut queues| queues.resize.take());
            if let Some(resize) = resize {
                let outcome = super::endpoints::resize(&self.host, resize.size);
                if outcome.is_err() {
                    if let Some(diagnostics) = &self.session.diagnostics {
                        diagnostics.count(
                            pty_runtime_application::diagnostics::CounterKind::FailedOperations,
                            1,
                        );
                    }
                }
                if let Some(timing) = resize.timing {
                    timing.finish(outcome.is_ok());
                }
                let _ = resize.reply.send(outcome);
            }
        }
        if shutting
            || self
                .exit_at
                .is_some_and(|start| now.duration_since(start) >= self.session.limits.drain_timeout)
        {
            self.session.stop();
        }
        if self.session.reader_done.load(Ordering::Acquire) {
            if self.exit_at.is_some() || self.supervision_lost || shutting {
                self.guardian.request(Kind::Release);
            }
            // PTY draining must not wait behind helper exit: macOS session exit
            // can itself wait for pending terminal output. After the public reader
            // finishes, bounded discarded reads keep control-plane teardown live.
            if !self.guardian.complete() && self.discard_at.is_none_or(|deadline| now >= deadline) {
                super::guardian::discard(&mut self.host);
                self.discard_at = Some(now + Duration::from_millis(10));
            }
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
            .and_then(|queues| queues.input.front().map(|input| input.deadline));
        let fallback = self
            .guardian
            .needs_poll()
            .then(|| Instant::now() + Duration::from_millis(10));
        [cancel, drain, input, self.discard_at, fallback]
            .into_iter()
            .flatten()
            .min()
    }
}
impl Drop for OwnedProcess {
    fn drop(&mut self) {
        self.session.stop();
        self.session.finish_inputs(ProcessError::Closed);
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        self.guardian.cleanup(&mut self.host);
        if let Some(diagnostics) = &self.session.diagnostics {
            let kind = if self.guardian.cleanup_succeeded() && !self.supervision_lost {
                pty_runtime_application::diagnostics::CounterKind::CleanupCompleted
            } else {
                pty_runtime_application::diagnostics::CounterKind::CleanupFailed
            };
            diagnostics.count(kind, 1);
        }
        self.session.release_wakes();
        let timing = self
            .session
            .cancel_timing
            .lock()
            .ok()
            .and_then(|mut pending| pending.take());
        drop(timing);
    }
}
