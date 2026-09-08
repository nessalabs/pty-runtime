use super::{RuntimeError, SessionContext};
use pty_runtime_domain::{SessionId, SessionLifetime};
use std::sync::Arc;

/// Atomic session registration, lookup and explicit removal boundary.
/// Implementations must never expose a mutable map or silently replace an entry.
pub trait ISessionRepository: Send + Sync {
    /// Insert once if ID absent and registry capacity permits, including finished entries.
    fn register(
        &self,
        id: SessionId,
        context: Arc<SessionContext>,
        capacity: usize,
    ) -> Result<(), RuntimeError>;
    /// Return the same context for a registered lifetime.
    fn lookup(&self, id: &SessionId) -> Result<Arc<SessionContext>, RuntimeError>;
    /// Remove only the matching finished lifetime; never a replacement with the same ID.
    fn remove_finished(
        &self,
        id: &SessionId,
        lifetime: SessionLifetime,
    ) -> Result<(), RuntimeError>;
    /// Roll back only the matching failed spawn reservation.
    fn rollback_spawn(&self, id: &SessionId, lifetime: SessionLifetime);
}
