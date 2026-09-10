use crate::{RuntimeError, RuntimeOptions, Session, SessionOptions, StorageOptions};
use pty_runtime_application::runtime as application;
use pty_runtime_domain::{SessionId, process::CommandSpec};
use pty_runtime_infrastructure::{
    identity::next_owner_identity, process::UnixProcessBackend, registry::MemorySessionRepository,
};
use std::{path::PathBuf, sync::Arc};

/// Long-lived owner of real PTY processes. Session/attachment handles do not own it.
/// Dropping this owner terminates its children even if handles remain elsewhere.
pub struct Runtime {
    inner: application::Runtime,
}
impl Runtime {
    /// Compose the default Unix process and in-memory registry adapters.
    /// Roots are a canonical launch-path policy, not a child filesystem sandbox.
    /// Call from a single-threaded window before other workers exist: helper-image
    /// staging forks, and platform atfork handlers can deadlock under locks.
    pub fn new(roots: Vec<PathBuf>, options: RuntimeOptions) -> Result<Self, RuntimeError> {
        Self::with_storage(roots, options, StorageOptions::default())
    }
    /// Configure the built-in private store; projected sessions park automatically.
    /// Raw-only builds do not instantiate terminal storage. Inject a terminal through
    /// `with_projection_adapters` to support projection without the Ghostty feature.
    pub fn with_storage(
        roots: Vec<PathBuf>,
        options: RuntimeOptions,
        storage: StorageOptions,
    ) -> Result<Self, RuntimeError> {
        options.validate()?;
        #[cfg(feature = "ghostty")]
        let services = crate::adapters::projection_services(&options, storage)?;
        #[cfg(not(feature = "ghostty"))]
        let _ = storage;
        let backend = Arc::new(UnixProcessBackend::new(roots, options.max_sessions)?);
        let inner = application::Runtime::new(
            next_owner_identity()?,
            options,
            Arc::new(MemorySessionRepository::default()),
            backend,
        )?;
        #[cfg(feature = "ghostty")]
        let inner = inner.with_projection(services)?;
        Ok(Self { inner })
    }
    /// Inject a raw process/repository composition; core models stay unchanged.
    /// Use `with_projection_adapters` when an authoritative terminal is also required.
    pub fn with_adapters(
        options: RuntimeOptions,
        repository: Arc<dyn crate::ports::ISessionRepository>,
        backend: Arc<dyn crate::ports::IProcessBackend>,
    ) -> Result<Self, RuntimeError> {
        Ok(Self {
            inner: application::Runtime::new(next_owner_identity()?, options, repository, backend)?,
        })
    }
    /// Inject all process, repository, terminal, storage, crypto and scheduling boundaries.
    /// The owner shuts down supplied workers and releases its services; do not share
    /// worker ownership with unrelated runtimes. No Ghostty feature is required.
    pub fn with_projection_adapters(
        options: RuntimeOptions,
        repository: Arc<dyn crate::ports::ISessionRepository>,
        backend: Arc<dyn crate::ports::IProcessBackend>,
        services: crate::ProjectionServices,
    ) -> Result<Self, RuntimeError> {
        Ok(Self {
            inner: application::Runtime::new(next_owner_identity()?, options, repository, backend)?
                .with_projection(services)?,
        })
    }
    /// Snapshot logical shared resource reservations, independent of opt-in timings.
    /// These are current quotas; OS RSS, stacks, helpers and allocator overhead are separate.
    pub fn resources(&self) -> crate::ResourceSnapshot {
        self.inner.resources()
    }
    /// Enable fixed shared timing counters for subsequently spawned sessions.
    /// Install before spawning the measured population; existing sessions keep their configuration.
    pub fn with_diagnostics(mut self, diagnostics: Arc<crate::RuntimeDiagnostics>) -> Self {
        self.inner = self.inner.with_diagnostics(diagnostics);
        self
    }
    /// Spawn once under a caller ID; retained completed IDs reject duplicate spawn.
    pub fn spawn(
        &self,
        id: SessionId,
        command: &CommandSpec,
        options: SessionOptions,
    ) -> Result<Session, RuntimeError> {
        self.inner.spawn(id, command, options)
    }
    /// Reconnect to the same registered lifetime without executing another child.
    pub fn lookup(&self, id: &SessionId) -> Result<Session, RuntimeError> {
        self.inner.lookup(id)
    }
    /// Release a finished registry entry explicitly; old handles keep their old lifetime.
    pub fn forget(&self, id: &SessionId) -> Result<(), RuntimeError> {
        self.inner.forget(id)
    }
    /// Synchronously reject spawn, terminate and reap owned children, and join readers.
    pub fn shutdown(&self) {
        self.inner.shutdown();
    }
}
