use crate::{runtime::quota::Quota, scheduling::ICapacitySignal};
use pty_runtime_domain::projection::{ProjectionError, ProjectionLimits};
use std::sync::{Arc, Mutex};

/// One runtime's projection reservations, independent of replay and provider quotas.
pub struct ProjectionBudgets {
    pub(super) limits: ProjectionLimits,
    pub(super) journal_bytes: Arc<Quota>,
    pub(super) journal_slots: Arc<Quota>,
    pub(super) transfer_observers: Arc<Quota>,
    pub(super) staging_bytes: Arc<Quota>,
    pub(super) staging_slots: Arc<Quota>,
    pub(super) resident: Arc<Quota>,
    pub(super) checkpoints: Arc<Quota>,
    pub(super) stored: Arc<Quota>,
    pub(super) stored_slots: Arc<Quota>,
    pub(super) unreclaimed: Mutex<Vec<super::state::UnreclaimedSource>>,
    pub(super) views: Arc<Quota>,
    pub(super) requests: Arc<Quota>,
}
impl ProjectionBudgets {
    /// Copy current shared admission reservations without enumerating sessions.
    pub fn resources(&self) -> crate::diagnostics::ProjectionResources {
        crate::diagnostics::ProjectionResources {
            journal_bytes: self.journal_bytes.usage(),
            journal_slots: self.journal_slots.usage(),
            transfer_observers: self.transfer_observers.usage(),
            staging_bytes: self.staging_bytes.usage(),
            staging_slots: self.staging_slots.usage(),
            native_reservations: self.resident.usage(),
            checkpoint_buffers: self.checkpoints.usage(),
            stored_bytes: self.stored.usage(),
            stored_slots: self.stored_slots.usage(),
            views: self.views.usage(),
            requests: self.requests.usage(),
        }
    }
    /// Validate finite global limits. Each result/pin keeps its reservation until dropped.
    pub fn new(limits: ProjectionLimits) -> Result<Arc<Self>, ProjectionError> {
        let limits = limits.validate()?;
        let mut unreclaimed = Vec::new();
        unreclaimed
            .try_reserve_exact(limits.stored_slots)
            .map_err(|_| ProjectionError::Capacity)?;
        Ok(Arc::new(Self {
            limits,
            journal_bytes: Arc::new(Quota::new(limits.journal_bytes)),
            journal_slots: Arc::new(Quota::new(limits.journal_slots)),
            transfer_observers: Arc::new(Quota::new(limits.transfer_observers)),
            staging_bytes: Arc::new(Quota::new(limits.staging_bytes)),
            staging_slots: Arc::new(Quota::new(limits.staging_slots)),
            resident: Arc::new(Quota::new(limits.resident_bytes)),
            checkpoints: Arc::new(Quota::new(limits.checkpoint_bytes)),
            stored: Arc::new(Quota::new(limits.stored_bytes)),
            stored_slots: Arc::new(Quota::new(limits.stored_slots)),
            unreclaimed: Mutex::new(unreclaimed),
            views: Arc::new(Quota::new(limits.view_bytes)),
            requests: Arc::new(Quota::new(limits.request_slots)),
        }))
    }
}
impl ProjectionBudgets {
    /// Sources whose finite cleanup retries failed. Their bytes and identity slots
    /// remain reserved across forget/new-session admission until this runtime ends.
    pub fn unreclaimed_sources(&self) -> usize {
        self.unreclaimed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }
    /// Conservative original byte reservations retained for failed deletions.
    pub fn unreclaimed_reserved_bytes(&self) -> usize {
        self.unreclaimed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|s| s._disk.bytes.count)
            .sum()
    }
}
pub(super) struct IoMemory {
    pub plain: Lease,
    pub _protected: Lease,
}
impl IoMemory {
    pub fn acquire(
        budgets: &ProjectionBudgets,
        plain: usize,
        protected: usize,
    ) -> Result<Self, ProjectionError> {
        let plain = Lease::shared(budgets.checkpoints.clone(), plain)?;
        let protected = Lease::shared(budgets.checkpoints.clone(), protected)?;
        Ok(Self {
            plain,
            _protected: protected,
        })
    }
}
pub(super) struct DiskLease {
    pub bytes: Lease,
    pub _slots: Lease,
}
impl DiskLease {
    pub fn acquire(budgets: &ProjectionBudgets, bytes: usize) -> Result<Self, ProjectionError> {
        let slots = Lease::shared(budgets.stored_slots.clone(), 1)?;
        let bytes = Lease::shared(budgets.stored.clone(), bytes)?;
        Ok(Self {
            bytes,
            _slots: slots,
        })
    }
}

pub(super) struct Lease {
    first: Arc<Quota>,
    second: Option<Arc<Quota>>,
    count: usize,
}
impl Lease {
    pub fn shared(first: Arc<Quota>, count: usize) -> Result<Self, ProjectionError> {
        if !first.acquire(count) {
            return Err(ProjectionError::Capacity);
        }
        Ok(Self {
            first,
            second: None,
            count,
        })
    }
    pub fn shared_and_local(
        first: Arc<Quota>,
        second: Arc<Quota>,
        count: usize,
    ) -> Result<Self, ProjectionError> {
        let mut lease = Self::shared(first, count)?;
        if !second.acquire(count) {
            return Err(ProjectionError::Capacity);
        }
        lease.second = Some(second);
        Ok(lease)
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.first.release(self.count);
        if let Some(second) = &self.second {
            second.release(self.count);
        }
    }
}
pub(super) struct StagingLease {
    pub timing: Option<crate::diagnostics::Timing>,
    pub bytes: Option<Lease>,
    pub slots: Option<Lease>,
    pub signal: Arc<dyn ICapacitySignal>,
}
impl Drop for StagingLease {
    fn drop(&mut self) {
        drop(self.bytes.take());
        drop(self.slots.take());
        self.signal.notify();
    }
}
