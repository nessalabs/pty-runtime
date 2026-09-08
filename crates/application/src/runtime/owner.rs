use super::{
    ISessionRepository, RuntimeError, RuntimeOptions, Session, SessionContext, SessionOptions,
    context::Events, lifecycle::AdmissionGate, projected::ProjectionRuntime, quota::Quota,
};
use crate::{process::IProcessBackend, projection::ProjectionServices};
use pty_runtime_domain::{
    SessionId,
    process::{CommandSpec, DrainOutcome},
    projection::ProjectionError,
    terminal::TerminalError,
};
use std::sync::{Arc, Mutex};

/// Application owner independent of transports. Dropping it shuts down its backend.
/// This type is intentionally not Clone; sessions do not extend owner lifetime.
pub struct Runtime {
    diagnostics: Option<Arc<crate::diagnostics::RuntimeDiagnostics>>,
    owner: u64,
    options: RuntimeOptions,
    lifecycle: AdmissionGate,
    shutdown: Mutex<()>,
    projection: Option<ProjectionRuntime>,
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
            diagnostics: None,
            owner,
            observers: Arc::new(Quota::new(options.max_observers)),
            replay: Arc::new(Quota::new(options.replay_bytes)),
            input_bytes: Arc::new(Quota::new(options.input_bytes)),
            input_slots: Arc::new(Quota::new(options.input_slots)),
            options,
            lifecycle: AdmissionGate::new(),
            shutdown: Mutex::new(()),
            projection: None,
            repository,
            backend,
        })
    }
    /// Snapshot current shared reservations, including consumer-retained buffers.
    pub fn resources(&self) -> crate::diagnostics::ResourceSnapshot {
        crate::diagnostics::ResourceSnapshot {
            observers: self.observers.usage(),
            replay_capacity: self.replay.usage(),
            input_bytes: self.input_bytes.usage(),
            input_slots: self.input_slots.usage(),
            projection: self.projection.as_ref().map(ProjectionRuntime::resources),
        }
    }
    /// Enable bounded shared diagnostics for subsequently spawned sessions.
    pub fn with_diagnostics(
        mut self,
        diagnostics: Arc<crate::diagnostics::RuntimeDiagnostics>,
    ) -> Self {
        self.diagnostics = Some(diagnostics);
        self
    }
    /// Compose terminal/storage/scheduling adapters for subsequently projected sessions.
    /// This runtime owns their worker lifetime and closes them during shutdown.
    pub fn with_projection(mut self, services: ProjectionServices) -> Result<Self, RuntimeError> {
        if self.projection.is_some() {
            return Err(ProjectionError::InvalidConfiguration.into());
        }
        self.projection = Some(ProjectionRuntime::new(
            services,
            self.options.projection,
            self.options.max_sessions,
        )?);
        Ok(self)
    }
    /// Reserve identity before executing the child. Early callbacks target the reserved context.
    /// Registration and process creation have rollback; completed IDs are never implicitly reused.
    pub fn spawn(
        &self,
        id: SessionId,
        command: &CommandSpec,
        options: SessionOptions,
    ) -> Result<Session, RuntimeError> {
        options.validate(&self.options)?;
        if options.projection.is_some() && self.projection.is_none() {
            return Err(ProjectionError::Terminal(TerminalError::Unsupported).into());
        }
        let (lifetime, _admission) = self.lifecycle.admit(self.owner)?;
        let context = Arc::new(
            SessionContext::new(
                lifetime,
                options.clone(),
                self.observers.clone(),
                self.replay.clone(),
                self.options.output_page_bytes,
                self.input_bytes.clone(),
                self.input_slots.clone(),
            )
            .with_diagnostics(self.diagnostics.clone()),
        );
        self.repository
            .register(id.clone(), context.clone(), self.options.max_sessions)?;
        if let (Some(owner), Some(policy)) = (&self.projection, options.projection) {
            match owner.create(
                lifetime,
                policy,
                self.input_bytes.clone(),
                self.input_slots.clone(),
            ) {
                Ok(projection) => {
                    *context
                        .projection
                        .lock()
                        .map_err(|_| RuntimeError::Internal)? = Some(projection);
                }
                Err(error) => {
                    context.record(|state| {
                        state.status.record_admission_failure(error);
                        let _ = state.status.record_drain(DrainOutcome::Eof);
                    });
                    self.repository.rollback_spawn(&id, lifetime);
                    return Err(error);
                }
            }
        }
        let events = Arc::new(Events(Arc::downgrade(&context)));
        match self
            .backend
            .spawn(command, options.size, lifetime, options.process, events)
        {
            Ok(process) => {
                *context.process.lock().map_err(|_| RuntimeError::Internal)? =
                    Some(process.clone());
                if let Some(projection) = context.projection()? {
                    // Binding failures remain independent projection status; the real child
                    // stays owned and available through raw I/O and lifecycle controls.
                    let _ = projection.bind_process(process.clone());
                }
                let closing = self.lifecycle.closing();
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
                if let (Some(owner), Ok(Some(projection))) =
                    (&self.projection, context.projection())
                {
                    let _ = owner.close(&projection);
                }
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
        if context.completion()?.is_none() {
            return Err(RuntimeError::NotFinished);
        }
        if let (Some(owner), Some(projection)) = (&self.projection, context.projection()?) {
            owner.close(&projection)?;
        }
        self.repository.remove_finished(id, context.lifetime())
    }
    /// Reject new spawns then terminate/reap all admitted processes through the backend.
    /// This is a blocking ownership operation, independent of async caller wait cancellation.
    pub fn shutdown(&self) {
        let _shutdown = self.shutdown.lock().unwrap_or_else(|e| e.into_inner());
        self.lifecycle.begin_shutdown();
        self.backend.shutdown();
        self.lifecycle.wait_for_spawns();
        if let Some(projection) = &self.projection {
            projection.shutdown();
        }
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        self.lifecycle.begin_shutdown();
        self.backend.shutdown_now();
        self.lifecycle.wait_for_spawns();
        if let Some(projection) = &self.projection {
            projection.shutdown();
        }
    }
}
