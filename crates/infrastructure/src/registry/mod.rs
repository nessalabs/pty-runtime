//! Atomic in-memory registry adapter, with no persistence claim.
use pty_runtime_application::runtime::{ISessionRepository, RuntimeError, SessionContext};
use pty_runtime_domain::{SessionId, SessionLifetime};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

/// Bounded registry. Completed IDs stay reserved until explicitly removed.
#[derive(Default)]
pub struct MemorySessionRepository {
    entries: Mutex<HashMap<SessionId, Arc<SessionContext>>>,
}
impl ISessionRepository for MemorySessionRepository {
    fn register(
        &self,
        id: SessionId,
        context: Arc<SessionContext>,
        capacity: usize,
    ) -> Result<(), RuntimeError> {
        let mut entries = self.entries.lock().map_err(|_| RuntimeError::Internal)?;
        if entries.contains_key(&id) {
            return Err(RuntimeError::ExistingSession);
        }
        if entries.len() >= capacity {
            return Err(RuntimeError::Capacity);
        }
        entries.insert(id, context);
        Ok(())
    }
    fn lookup(&self, id: &SessionId) -> Result<Arc<SessionContext>, RuntimeError> {
        self.entries
            .lock()
            .map_err(|_| RuntimeError::Internal)?
            .get(id)
            .cloned()
            .ok_or(RuntimeError::MissingSession)
    }
    fn remove_finished(
        &self,
        id: &SessionId,
        lifetime: SessionLifetime,
    ) -> Result<(), RuntimeError> {
        let context = self.lookup(id)?;
        if context.lifetime() != lifetime {
            return Err(RuntimeError::MissingSession);
        }
        if context.completion()?.is_none() {
            return Err(RuntimeError::NotFinished);
        }
        let mut entries = self.entries.lock().map_err(|_| RuntimeError::Internal)?;
        if entries
            .get(id)
            .is_some_and(|current| current.lifetime() == lifetime)
        {
            entries.remove(id);
            Ok(())
        } else {
            Err(RuntimeError::MissingSession)
        }
    }
    fn rollback_spawn(&self, id: &SessionId, lifetime: SessionLifetime) {
        if let Ok(mut entries) = self.entries.lock() {
            if entries
                .get(id)
                .is_some_and(|current| current.lifetime() == lifetime)
            {
                entries.remove(id);
            }
        }
    }
}
