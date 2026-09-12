//! Injected boundaries and immutable configuration for one projection.
//!
//! Everything here is either fixed for the projection's life (options, the
//! engine's compatibility identity, the protected-size limit) or is an external
//! collaborator that is released during cleanup (the service bundle, the
//! scheduler registration, the bound process).
//!
//! Grouping them keeps the slots private, so nothing outside this file can
//! swap a collaborator or observe a released one. It does NOT constrain the
//! coordinator's own methods: they take `&self`, so every one of them still has
//! `self.wiring` in scope. Narrowing that requires moving methods off the
//! coordinator, not fields into a struct.
use super::{ProjectionError, ProjectionOptions, ProjectionServices};
use crate::{process::IProcessSession, scheduling::IWorkHandle};
use pty_runtime_domain::terminal::CompatibilityId;
use std::sync::{Arc, Mutex};

/// Take a leaf lock, registering its tier for as long as the mutex is held.
///
/// The registration travels inside the returned guard rather than living in
/// this function: a local here would be dropped when `leaf` returns, while the
/// caller still holds the mutex, so nesting *under* a leaf would go undetected.
fn leaf<T>(slot: &Mutex<T>) -> super::queue::Guarded<'_, T> {
    #[cfg(test)]
    let _tier = super::tier::enter(super::tier::Tier::Leaf);
    super::queue::Guarded::new(
        slot.lock().unwrap_or_else(|e| e.into_inner()),
        #[cfg(test)]
        _tier,
    )
}

/// What is fixed for the projection's whole life.
///
/// Separate from [`Wiring`] because the two have different reasons to change:
/// this is decided once at creation and then only read, while the slots in
/// `Wiring` exist to be released during cleanup. Holding them in one type meant
/// every reader of a bound also went through the release protocol's mutexes.
pub(super) struct ProjectionConfig {
    /// Validated per-session bounds and parking policy.
    pub options: ProjectionOptions,
    /// Validated once at creation and cloned into every descriptor, so no later
    /// path has to re-check the identity or invent a fallback for it.
    pub compatibility: CompatibilityId,
    /// Ciphertext bound this protector reports for the configured plaintext cap.
    pub protected_bytes: usize,
}

/// The injected collaborators, each released during cleanup.
pub(super) struct Wiring {
    services: Mutex<Option<ProjectionServices>>,
    handle: Mutex<Option<Arc<dyn IWorkHandle>>>,
    process: Mutex<Option<Arc<dyn IProcessSession>>>,
}

impl Wiring {
    /// Hold the injected collaborators for one projection. The scheduler
    /// registration and the process are bound later, once they exist.
    pub fn new(services: ProjectionServices) -> Self {
        Self {
            services: Mutex::new(Some(services)),
            handle: Mutex::new(None),
            process: Mutex::new(None),
        }
    }

    /// External collaborators, or `Closed` once cleanup has released them.
    pub fn services(&self) -> Result<ProjectionServices, ProjectionError> {
        leaf(&self.services).clone().ok_or(ProjectionError::Closed)
    }

    /// Publish the scheduler registration made after construction.
    pub fn attach_handle(&self, handle: Arc<dyn IWorkHandle>) {
        *leaf(&self.handle) = Some(handle);
    }

    /// Request one more worker run. Coalesced by the scheduler.
    pub fn wake(&self) -> Result<(), ProjectionError> {
        // Scoped so the guard, and its tier registration, are released before
        // the scheduler runs: waking may re-enter this projection.
        let handle = { leaf(&self.handle).clone() }.ok_or(ProjectionError::Worker)?;
        handle.wake().map_err(|_| ProjectionError::Worker)
    }

    /// The registration itself, for waking from a blocking worker thread.
    pub fn handle(&self) -> Option<Arc<dyn IWorkHandle>> {
        leaf(&self.handle).clone()
    }

    /// Bind exactly one admitted process; a second binding is rejected.
    ///
    /// Binding and the caller's residency check cannot be one atomic step
    /// without taking the admission lock under this one, which would invert the
    /// lock order. The caller instead binds and then re-checks, using
    /// [`Self::unbind_process`] to undo the bind if closure won the race.
    pub fn bind_process(&self, process: Arc<dyn IProcessSession>) -> Result<(), ProjectionError> {
        let mut slot = leaf(&self.process);
        if slot.is_some() {
            return Err(ProjectionError::InvalidConfiguration);
        }
        *slot = Some(process);
        Ok(())
    }

    /// Undo a bind that raced a completing cleanup, so the slot cannot outlive
    /// the cleanup that was supposed to clear it.
    pub fn unbind_process(&self) {
        let taken = leaf(&self.process).take();
        drop(taken);
    }

    /// The bound process, if a child has been admitted yet.
    pub fn process(&self) -> Option<Arc<dyn IProcessSession>> {
        leaf(&self.process).clone()
    }

    /// Release one collaborator. Cleanup drops these in an order the caller
    /// controls, so they are surrendered individually rather than together.
    pub fn take_process(&self) -> Option<Arc<dyn IProcessSession>> {
        leaf(&self.process).take()
    }

    /// Release the service bundle; every later `services()` reports `Closed`.
    pub fn take_services(&self) -> Option<ProjectionServices> {
        leaf(&self.services).take()
    }

    /// Release the scheduler registration; every later `wake()` reports `Worker`.
    pub fn take_handle(&self) -> Option<Arc<dyn IWorkHandle>> {
        leaf(&self.handle).take()
    }
}

/// Run `under` while a real leaf mutex is held, for the lock-order tests.
///
/// Leaves are leaves precisely because production acquires nothing while one is
/// held, so no production path can demonstrate the span of a leaf's tier
/// registration — the holder has to be synthetic. It still goes through the
/// real [`leaf`] helper, which is the thing under test: with the registration
/// living in a local inside `leaf`, it would be dropped before `under` runs and
/// a nesting under a leaf would go unnoticed.
#[cfg(test)]
pub(super) fn hold_leaf(under: impl FnOnce()) {
    static SLOT: Mutex<()> = Mutex::new(());
    let _held = leaf(&SLOT);
    under();
}
