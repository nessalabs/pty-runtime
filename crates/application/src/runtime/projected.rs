use super::{RuntimeError, quota::Quota};
use crate::projection::{ProjectionBudgets, ProjectionCoordinator, ProjectionServices};
use pty_runtime_domain::{
    SessionLifetime,
    projection::{ProjectionError, ProjectionLimits, ProjectionOptions, Residency},
};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

enum Slot {
    Vacant,
    Reserved,
    Owned(Arc<ProjectionCoordinator>),
}
/// Resource ownership inventory, separate from caller-selected session repository.
/// It retains the same coordinator objects until close; no domain state is mirrored.
pub(super) struct ProjectionRuntime {
    services: Mutex<Option<ProjectionServices>>,
    budgets: Arc<ProjectionBudgets>,
    owned: Mutex<Vec<Slot>>,
}
impl ProjectionRuntime {
    pub fn new(
        services: ProjectionServices,
        limits: ProjectionLimits,
        max_sessions: usize,
    ) -> Result<Self, RuntimeError> {
        let budgets = ProjectionBudgets::new(limits)?;
        let mut owned = Vec::new();
        owned
            .try_reserve_exact(max_sessions)
            .map_err(|_| RuntimeError::Capacity)?;
        owned.resize_with(max_sessions, || Slot::Vacant);
        Ok(Self {
            services: Mutex::new(Some(services)),
            budgets,
            owned: Mutex::new(owned),
        })
    }
    pub fn create(
        &self,
        lifetime: SessionLifetime,
        options: ProjectionOptions,
        input_bytes: Arc<Quota>,
        input_slots: Arc<Quota>,
    ) -> Result<Arc<ProjectionCoordinator>, RuntimeError> {
        // Reserve teardown ownership before any callback or native allocation can occur.
        let reservation = self.reserve()?;
        let projection = ProjectionCoordinator::create(
            lifetime,
            options,
            self.services()?,
            self.budgets.clone(),
            input_bytes,
            input_slots,
        )?;
        reservation.publish(projection.clone());
        Ok(projection)
    }
    fn reserve(&self) -> Result<Reservation<'_>, RuntimeError> {
        let mut owned = self.owned.lock().map_err(|_| RuntimeError::Internal)?;
        for slot in owned.iter_mut() {
            if matches!(slot, Slot::Owned(model) if model.status().residency == Residency::Closed) {
                *slot = Slot::Vacant;
            }
        }
        let index = owned
            .iter()
            .position(|slot| matches!(slot, Slot::Vacant))
            .ok_or(RuntimeError::Capacity)?;
        owned[index] = Slot::Reserved;
        Ok(Reservation {
            owner: self,
            index,
            published: false,
        })
    }
    fn services(&self) -> Result<ProjectionServices, RuntimeError> {
        self.services
            .lock()
            .map_err(|_| RuntimeError::Internal)?
            .clone()
            .ok_or(RuntimeError::Closed)
    }
    pub fn close(&self, projection: &Arc<ProjectionCoordinator>) -> Result<(), RuntimeError> {
        if let Some(outcome) = projection.close_outcome() {
            return outcome.map_err(Into::into);
        }
        let services = self.services()?;
        match projection.close() {
            // Closure remains durable even when normal observation slots are occupied.
            Ok(_) | Err(ProjectionError::Capacity) => (),
            Err(error) => return Err(error.into()),
        }
        loop {
            let generation = services.capacity.generation();
            if let Some(outcome) = projection.close_outcome() {
                return outcome.map_err(Into::into);
            }
            services
                .capacity
                .wait_after(generation, Instant::now() + Duration::from_secs(1));
        }
    }
    pub fn shutdown(&self) {
        // Runtime admission has closed and all in-flight spawns have finished before this call.
        let owned = {
            let mut owned = self.owned.lock().unwrap_or_else(|e| e.into_inner());
            std::mem::take(&mut *owned)
        };
        for slot in &owned {
            if let Slot::Owned(projection) = slot {
                let _ = self.close(projection);
            }
        }
        let services = self
            .services
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(services) = services {
            services.scheduler.shutdown();
            services.blocking.shutdown();
            // A failed injected scheduler cannot execute a close wake. After both
            // pools have joined, no external/native work may still own these models.
            for slot in &owned {
                if let Slot::Owned(projection) = slot {
                    if projection.status().residency != Residency::Closed {
                        projection.finish_after_shutdown();
                    }
                }
            }
        }
    }
}
struct Reservation<'a> {
    owner: &'a ProjectionRuntime,
    index: usize,
    published: bool,
}
impl Reservation<'_> {
    fn publish(mut self, projection: Arc<ProjectionCoordinator>) {
        self.owner.owned.lock().unwrap_or_else(|e| e.into_inner())[self.index] =
            Slot::Owned(projection);
        self.published = true;
    }
}
impl Drop for Reservation<'_> {
    fn drop(&mut self) {
        if !self.published {
            self.owner.owned.lock().unwrap_or_else(|e| e.into_inner())[self.index] = Slot::Vacant;
        }
    }
}
