use std::path::PathBuf;

/// Default encrypted checkpoint storage configuration. Both this disk limit and
/// the runtime's independent projection budgets apply to every accepted object.
#[derive(Clone)]
pub struct StorageOptions {
    /// Parent for a new private opaque namespace; None uses the OS temporary directory.
    /// The parent must remain trusted against concurrent namespace replacement.
    pub parent: Option<PathBuf>,
    /// Maximum logical ciphertext bytes, including abandoned failed writes.
    pub max_bytes: usize,
}
impl Default for StorageOptions {
    fn default() -> Self {
        Self {
            parent: None,
            max_bytes: 1024 * 1024 * 1024,
        }
    }
}
impl std::fmt::Debug for StorageOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StorageOptions")
            .field("configured_parent", &self.parent.is_some())
            .field("max_bytes", &self.max_bytes)
            .finish()
    }
}

#[cfg(feature = "ghostty")]
pub(crate) fn projection_services(
    options: &crate::RuntimeOptions,
    storage: StorageOptions,
) -> Result<pty_runtime_application::projection::ProjectionServices, crate::RuntimeError> {
    use pty_runtime_application::projection::ProjectionServices;
    use pty_runtime_domain::projection::ProjectionError;
    use pty_runtime_infrastructure::{
        checkpoint::{CheckpointProtector, FileCheckpointStore},
        scheduling::{
            BoundedBlockingExecutor, CondvarCapacitySignal, MonotonicSystemClock, StdWorkScheduler,
        },
        terminal::GhosttyTerminalFactory,
    };
    use std::sync::Arc;
    let store = FileCheckpointStore::temporary(storage.parent.as_deref(), storage.max_bytes)
        .map_err(ProjectionError::from)?;
    let protector =
        CheckpointProtector::new(options.projection.checkpoint_bytes.min(512 * 1024 * 1024))
            .map_err(ProjectionError::from)?;
    let scheduler =
        StdWorkScheduler::new(options.max_sessions).map_err(|_| ProjectionError::Worker)?;
    let blocking = BoundedBlockingExecutor::new(options.max_sessions, 2)
        .map_err(|_| ProjectionError::Worker)?;
    Ok(ProjectionServices {
        terminal: Arc::new(GhosttyTerminalFactory),
        clock: Arc::new(MonotonicSystemClock::default()),
        scheduler: Arc::new(scheduler),
        blocking: Arc::new(blocking),
        capacity: Arc::new(CondvarCapacitySignal::default()),
        store: Arc::new(store),
        protector: Arc::new(protector),
    })
}
