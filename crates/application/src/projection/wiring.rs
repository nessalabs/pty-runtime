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
use std::sync::{Arc, Mutex, MutexGuard};

/// Take a leaf lock, registering its tier in test builds. Nothing may be
/// acquired while one of these is held.
fn leaf<T>(slot: &Mutex<T>) -> MutexGuard<'_, T> {
    #[cfg(test)]
    let _tier = super::tier::enter(super::tier::Tier::Leaf);
    slot.lock().unwrap_or_else(|e| e.into_inner())
}

pub(super) struct Wiring {
    /// Validated per-session bounds and parking policy.
    pub options: ProjectionOptions,
    /// Validated once at creation and cloned into every descriptor, so no later
    /// path has to re-check the identity or invent a fallback for it.
    pub compatibility: CompatibilityId,
    /// Ciphertext bound this protector reports for the configured plaintext cap.
    pub protected_bytes: usize,
    services: Mutex<Option<ProjectionServices>>,
    handle: Mutex<Option<Arc<dyn IWorkHandle>>>,
    process: Mutex<Option<Arc<dyn IProcessSession>>>,
}

impl Wiring {
    /// Hold the injected boundaries for one projection. The scheduler
    /// registration and the process are bound later, once they exist.
    pub fn new(
        options: ProjectionOptions,
        compatibility: CompatibilityId,
        protected_bytes: usize,
        services: ProjectionServices,
    ) -> Self {
        Self {
            options,
            compatibility,
            protected_bytes,
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
        let handle = self
            .handle
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or(ProjectionError::Worker)?;
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
        let taken = self
            .process
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
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

/// Failure-injection seams used only by this crate's tests.
///
/// Production code never replaces a collaborator after construction. Cleanup
/// evidence needs no seam: `services()` reports `Closed` and `handle()` /
/// `process()` report `None` once released.
#[cfg(test)]
impl Wiring {
    /// Swap an injected collaborator mid-life to exercise a provider fault.
    pub fn inject_services(&self, change: impl FnOnce(&mut ProjectionServices)) {
        if let Some(services) = self
            .services
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
        {
            change(services);
        }
    }

    /// Install or clear the registration to exercise scheduler loss.
    pub fn inject_handle(&self, handle: Option<Arc<dyn IWorkHandle>>) {
        *leaf(&self.handle) = handle;
    }
}
