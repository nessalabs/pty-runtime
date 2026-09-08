use super::{AttachPosition, Attachment, Completion, RuntimeError, SessionContext, SessionStatus};
use crate::process::ProcessOperation;
use pty_runtime_domain::{
    SessionLifetime,
    process::{ProcessError, WriteOutcome},
    terminal::TerminalSize,
};
use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

/// Control/observation handle for one lifetime. Drop never kills its process.
#[derive(Clone)]
pub struct Session {
    pub(crate) context: Arc<SessionContext>,
}
impl Session {
    /// Runtime-issued lifetime that binds every replay cursor.
    pub fn lifetime(&self) -> SessionLifetime {
        self.context.lifetime
    }
    /// Diagnostic process ID; cannot be used for controls through this API.
    pub fn process_id(&self) -> Result<u32, RuntimeError> {
        Ok(self.context.process()?.process_id())
    }
    /// Actual known lifecycle facts.
    pub fn status(&self) -> Result<SessionStatus, RuntimeError> {
        self.context.status()
    }
    /// Attach an independently bounded observer without restarting the child.
    pub fn attach(&self, position: AttachPosition) -> Result<Attachment, RuntimeError> {
        Attachment::new(self.context.clone(), position)
    }
    /// Admit transient input before returning a wait. Dropping the wait does not resend/undo bytes.
    pub fn write(&self, bytes: &[u8]) -> Result<ProcessOperation<WriteOutcome>, RuntimeError> {
        let lease = super::quota::InputLease::acquire(
            self.context.input_bytes.clone(),
            self.context.input_slots.clone(),
            bytes.len(),
        )?;
        Ok(self
            .context
            .process()?
            .write_reserved(bytes, Some(Box::new(lease)))?)
    }
    /// Admit a coalesced termination sequence independent of input congestion.
    pub fn cancel(&self) -> Result<(), RuntimeError> {
        {
            let mut state = self
                .context
                .state
                .lock()
                .map_err(|_| RuntimeError::Internal)?;
            state.status.admit_cancel()?;
        }
        let process = self
            .context
            .process
            .lock()
            .map_err(|_| RuntimeError::Internal)?
            .clone();
        if let Some(process) = process {
            process.request_cancel()?;
        }
        Ok(())
    }
    /// Resize a raw PTY. Projected sessions must use `resize_projected` to preserve
    /// ordered model state and expose separate OS/model outcomes.
    pub fn resize(
        &self,
        size: TerminalSize,
    ) -> Result<ProcessOperation<Result<(), ProcessError>>, RuntimeError> {
        if self.context.options.projection.is_some() {
            return Err(pty_runtime_domain::projection::ProjectionError::Terminal(
                pty_runtime_domain::terminal::TerminalError::Unsupported,
            )
            .into());
        }
        Ok(self.context.process()?.resize(size)?)
    }
    /// Reserve a bounded completion waiter; cancelling its future does not cancel the child.
    pub fn wait(&self) -> Result<CompletionWait, RuntimeError> {
        let id = self.context.register_watcher()?;
        Ok(CompletionWait {
            context: self.context.clone(),
            id,
        })
    }
}

/// Wait for both actual process supervision and final drain outcome.
pub struct CompletionWait {
    context: Arc<SessionContext>,
    id: u64,
}
impl Future for CompletionWait {
    type Output = Result<Completion, RuntimeError>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Ok(mut state) = self.context.state.lock() else {
            return Poll::Ready(Err(RuntimeError::Internal));
        };
        if let Some(done) = state.status.completion() {
            return Poll::Ready(Ok(done));
        }
        if let Some(slot) = state.watchers.get_mut(&self.id) {
            *slot = Some(cx.waker().clone());
        }
        Poll::Pending
    }
}
impl Drop for CompletionWait {
    fn drop(&mut self) {
        self.context.remove_watcher(self.id);
    }
}
