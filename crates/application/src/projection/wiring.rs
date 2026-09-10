//! Injected boundaries and immutable configuration for one projection.
//!
//! Everything here is either fixed for the projection's life (options, the
//! engine's compatibility identity, the protected-size limit) or is an external
//! collaborator that is released during cleanup (the service bundle, the
//! scheduler registration, the bound process).
//!
//! Keeping them together means the state under the two ownership mutexes does
//! not also carry the wiring needed to act on it: a function given
//! `&mut NativeWorkspace` can drive the engine but cannot, for instance, reach
//! past it to wake the scheduler.
use super::{ProjectionError, ProjectionOptions, ProjectionServices};
use crate::{process::IProcessSession, scheduling::IWorkHandle};
use pty_runtime_domain::terminal::CompatibilityId;
use std::sync::{Arc, Mutex};

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
        self.services
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or(ProjectionError::Closed)
    }

    /// Publish the scheduler registration made after construction.
    pub fn attach_handle(&self, handle: Arc<dyn IWorkHandle>) {
        *self.handle.lock().unwrap_or_else(|e| e.into_inner()) = Some(handle);
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
        self.handle
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Bind exactly one admitted process; a second binding is rejected.
    pub fn bind_process(&self, process: Arc<dyn IProcessSession>) -> Result<(), ProjectionError> {
        let mut slot = self.process.lock().unwrap_or_else(|e| e.into_inner());
        if slot.is_some() {
            return Err(ProjectionError::InvalidConfiguration);
        }
        *slot = Some(process);
        Ok(())
    }

    /// The bound process, if a child has been admitted yet.
    pub fn process(&self) -> Option<Arc<dyn IProcessSession>> {
        self.process
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Release one collaborator. Cleanup drops these in an order the caller
    /// controls, so they are surrendered individually rather than together.
    pub fn take_process(&self) -> Option<Arc<dyn IProcessSession>> {
        self.process
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
    }

    /// Release the service bundle; every later `services()` reports `Closed`.
    pub fn take_services(&self) -> Option<ProjectionServices> {
        self.services
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
    }

    /// Release the scheduler registration; every later `wake()` reports `Worker`.
    pub fn take_handle(&self) -> Option<Arc<dyn IWorkHandle>> {
        self.handle.lock().unwrap_or_else(|e| e.into_inner()).take()
    }
}

/// Seams used only by this crate's tests.
///
/// Cleanup evidence and failure injection: production code neither inspects a
/// released slot nor swaps a collaborator after construction, so none of this
/// is compiled into the library.
#[cfg(test)]
impl Wiring {
    /// Whether cleanup has released the service bundle.
    pub fn services_released(&self) -> bool {
        self.services
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_none()
    }

    /// Whether cleanup has released the scheduler registration.
    pub fn handle_released(&self) -> bool {
        self.handle
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_none()
    }

    /// Whether cleanup has released the bound process.
    pub fn process_released(&self) -> bool {
        self.process
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_none()
    }

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
        *self.handle.lock().unwrap_or_else(|e| e.into_inner()) = handle;
    }
}
