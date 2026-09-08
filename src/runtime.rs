use crate::{RuntimeError, RuntimeOptions, Session, SessionOptions};
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
    pub fn new(roots: Vec<PathBuf>, options: RuntimeOptions) -> Result<Self, RuntimeError> {
        let backend = Arc::new(UnixProcessBackend::new(roots, options.max_sessions)?);
        let inner = application::Runtime::new(
            next_owner_identity()?,
            options,
            Arc::new(MemorySessionRepository::default()),
            backend,
        )?;
        Ok(Self { inner })
    }
    /// Inject process/repository implementations; core models stay unchanged.
    pub fn with_adapters(
        options: RuntimeOptions,
        repository: Arc<dyn crate::ports::ISessionRepository>,
        backend: Arc<dyn crate::ports::IProcessBackend>,
    ) -> Result<Self, RuntimeError> {
        Ok(Self {
            inner: application::Runtime::new(next_owner_identity()?, options, repository, backend)?,
        })
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
