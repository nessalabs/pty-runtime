use super::{
    ISessionRepository, RuntimeError, RuntimeOptions, Session, SessionContext, SessionOptions,
    context::Events, quota::Quota,
};
use crate::process::IProcessBackend;
use pty_runtime_domain::{SessionId, SessionLifetime, process::CommandSpec};
use std::sync::{Arc, Mutex};

struct Lifecycle {
    closing: bool,
    sequence: u64,
}
/// Application owner independent of transports. Dropping it shuts down its backend.
/// This type is intentionally not Clone; sessions do not extend owner lifetime.
pub struct Runtime {
    owner: u64,
    options: RuntimeOptions,
    lifecycle: Mutex<Lifecycle>,
    repository: Arc<dyn ISessionRepository>,
    backend: Arc<dyn IProcessBackend>,
    observers: Arc<Quota>,
    replay: Arc<Quota>,
    input_bytes: Arc<Quota>,
    input_slots: Arc<Quota>,
}
impl Runtime {
    /// Compose external implementations. Owner identity must be unique among live runtimes.
    pub fn new(
        owner: u64,
        options: RuntimeOptions,
        repository: Arc<dyn ISessionRepository>,
        backend: Arc<dyn IProcessBackend>,
    ) -> Result<Self, RuntimeError> {
        options.validate()?;
        Ok(Self {
            owner,
            observers: Arc::new(Quota::new(options.max_observers)),
            replay: Arc::new(Quota::new(options.replay_bytes)),
            input_bytes: Arc::new(Quota::new(options.input_bytes)),
            input_slots: Arc::new(Quota::new(options.input_slots)),
            options,
            lifecycle: Mutex::new(Lifecycle {
                closing: false,
                sequence: 0,
            }),
            repository,
            backend,
        })
    }
    /// Reserve identity before executing the child. Early callbacks target the reserved context.
    /// Registration and process creation have rollback; completed IDs are never implicitly reused.
    pub fn spawn(
        &self,
        id: SessionId,
        command: &CommandSpec,
        options: SessionOptions,
    ) -> Result<Session, RuntimeError> {
        options.process.validate()?;
        if options.max_observers == 0 {
            return Err(RuntimeError::Capacity);
        }
        let lifetime = {
            let mut state = self.lifecycle.lock().map_err(|_| RuntimeError::Internal)?;
            if state.closing {
                return Err(RuntimeError::Closed);
            }
            state.sequence = state
                .sequence
                .checked_add(1)
                .ok_or(RuntimeError::Capacity)?;
            SessionLifetime::new(self.owner, state.sequence)
        };
        let context = Arc::new(SessionContext::new(
            lifetime,
            options.clone(),
            self.observers.clone(),
            self.replay.clone(),
            self.options.output_page_bytes,
            self.input_bytes.clone(),
            self.input_slots.clone(),
        ));
        self.repository
            .register(id.clone(), context.clone(), self.options.max_sessions)?;
        let events = Arc::new(Events(Arc::downgrade(&context)));
        match self
            .backend
            .spawn(command, options.size, lifetime, options.process, events)
        {
            Ok(process) => {
                *context.process.lock().map_err(|_| RuntimeError::Internal)? =
                    Some(process.clone());
                let closing = self
                    .lifecycle
                    .lock()
                    .map_err(|_| RuntimeError::Internal)?
                    .closing;
                let cancelled = context.status()?.cancellation_requested;
                if closing || cancelled {
                    let _ = process.request_cancel();
                }
                Ok(Session { context })
            }
            Err(error) => {
                context.record(|state| {
                    state.status.record_failure(error);
                    let _ = state
                        .status
                        .record_drain(pty_runtime_domain::process::DrainOutcome::Failed(error));
                });
                self.repository.rollback_spawn(&id, lifetime);
                Err(error.into())
            }
        }
    }
    /// Return the existing lifetime rather than launching again.
    pub fn lookup(&self, id: &SessionId) -> Result<Session, RuntimeError> {
        Ok(Session {
            context: self.repository.lookup(id)?,
        })
    }
    /// Explicitly remove a finished entry. Existing handles remain tied to that old lifetime.
    pub fn forget(&self, id: &SessionId) -> Result<(), RuntimeError> {
        let context = self.repository.lookup(id)?;
        self.repository.remove_finished(id, context.lifetime())
    }
    /// Reject new spawns then terminate/reap all admitted processes through the backend.
    /// This is a blocking ownership operation, independent of async caller wait cancellation.
    pub fn shutdown(&self) {
        if let Ok(mut state) = self.lifecycle.lock() {
            state.closing = true;
        }
        self.backend.shutdown();
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        if let Ok(mut state) = self.lifecycle.lock() {
            state.closing = true;
        }
        self.backend.shutdown_now();
    }
}
