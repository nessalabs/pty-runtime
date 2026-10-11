use super::{ProjectionCoordinator, ProjectionError, Residency, budgets::Lease};
use pty_runtime_domain::terminal::program_status::{
    ProgramStatusPlan, ProgramStatusRecord, ProgramStatusSnapshot, ProgramStatusStore,
};
use std::fmt;

/// An owned program-status snapshot whose memory stays charged until this value is dropped.
/// Access is borrowed; explicitly copying records creates caller-owned memory.
pub struct ProjectedProgramStatus {
    snapshot: ProgramStatusSnapshot,
    _memory: Lease,
}
impl fmt::Debug for ProjectedProgramStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.snapshot.fmt(f)
    }
}
impl ProjectedProgramStatus {
    /// Record revision at the point this snapshot was copied.
    pub fn revision(&self) -> u64 {
        self.snapshot.revision
    }
    /// Bounded stored reports with inherited applications resolved at read time.
    pub fn records(&self) -> &[ProgramStatusRecord] {
        &self.snapshot.records
    }
}

/// Only the serialized worker mutates the store. Observers copy under this short
/// mutex; no native or provider call runs while it is held. It never nests the
/// admission lock. Native ownership is unnecessary for reads and process exit.
pub(super) struct ProgramStatusState {
    store: Option<ProgramStatusStore>,
    memory: Option<Lease>,
    exited: bool,
    drained: bool,
}
impl ProgramStatusState {
    pub fn new(limit: usize) -> Result<Self, ProjectionError> {
        Ok(Self {
            store: Some(
                ProgramStatusStore::new(limit)
                    .map_err(|_| ProjectionError::InvalidConfiguration)?,
            ),
            memory: None,
            exited: false,
            drained: false,
        })
    }
}
pub(super) enum StatusAdmissionError {
    Retry,
    Failed(ProjectionError),
}
pub(super) struct PreparedStatus {
    pub plan: ProgramStatusPlan,
    pub _scan_memory: Lease,
    pub record_memory: Option<Lease>,
    pub reply_memory: Lease,
}
impl ProjectionCoordinator {
    fn lock_program_status(&self) -> super::queue::Guarded<'_, ProgramStatusState> {
        #[cfg(test)]
        let tier = super::tier::enter(super::tier::Tier::Leaf);
        super::queue::Guarded::new(
            self.program_status
                .lock()
                .unwrap_or_else(|e| e.into_inner()),
            #[cfg(test)]
            tier,
        )
    }
    /// Take an admitted bounded copy without waking or restoring a parked model.
    /// The quota charge belongs to the returned value, including after session close.
    pub fn program_status(&self) -> Result<ProjectedProgramStatus, ProjectionError> {
        let status = self.status();
        if matches!(status.residency, Residency::Closing | Residency::Closed) {
            return Err(ProjectionError::Closed);
        }
        if let Some(error) = status.failure {
            return Err(error);
        }
        let state = self.lock_program_status();
        let store = state.store.as_ref().ok_or(ProjectionError::Closed)?;
        let memory = Lease::shared(
            self.quotas.shared.views.clone(),
            store.snapshot_reservation(),
        )?;
        Ok(ProjectedProgramStatus {
            snapshot: store.records(),
            _memory: memory,
        })
    }
    pub(super) fn prepare_status(
        &self,
        bytes: &[u8],
        reply_limit: usize,
    ) -> Result<PreparedStatus, StatusAdmissionError> {
        use StatusAdmissionError::{Failed, Retry};
        let state = self.lock_program_status();
        let store = state
            .store
            .as_ref()
            .ok_or(Failed(ProjectionError::Closed))?;
        let scratch = store
            .scan_reservation_for(bytes)
            .map_err(|_| Failed(ProjectionError::Capacity))?;
        let owned = if state.memory.is_some() {
            store.record_reservation()
        } else {
            0
        };
        let limit = self.quotas.shared.limits.view_bytes;
        if owned + scratch + reply_limit > limit {
            return Err(Failed(ProjectionError::Capacity));
        }
        let scan_memory =
            Lease::shared(self.quotas.shared.views.clone(), scratch).map_err(|_| Retry)?;
        let plan = store
            .scan(bytes)
            .map_err(|_| Failed(ProjectionError::Capacity))?;
        let reply_reservation = if plan.query_offsets().is_empty() {
            reply_limit
        } else {
            reply_limit * 2
        };
        let record_reservation = if state.memory.is_none() && plan.retained_bytes() > 0 {
            store.record_reservation()
        } else {
            0
        };
        if owned + scratch + reply_reservation + record_reservation > limit {
            return Err(Failed(ProjectionError::Capacity));
        }
        let record_memory = if record_reservation > 0 {
            Some(
                Lease::shared(self.quotas.shared.views.clone(), record_reservation)
                    .map_err(|_| Retry)?,
            )
        } else {
            None
        };
        let reply_memory = Lease::shared(self.quotas.shared.views.clone(), reply_reservation)
            .map_err(|_| Retry)?;
        Ok(PreparedStatus {
            plan,
            _scan_memory: scan_memory,
            record_memory,
            reply_memory,
        })
    }
    pub(super) fn commit_status(
        &self,
        plan: ProgramStatusPlan,
        record_memory: Option<Lease>,
    ) -> Result<(), ProjectionError> {
        let mut state = self.lock_program_status();
        state
            .store
            .as_mut()
            .ok_or(ProjectionError::Closed)?
            .commit(plan)
            .map_err(|_| ProjectionError::Worker)?;
        if record_memory.is_some() {
            state.memory = record_memory;
        }
        Ok(())
    }
    pub(super) fn apply_program_exit(&self) -> Result<(), ProjectionError> {
        let mut state = self.lock_program_status();
        state
            .store
            .as_mut()
            .ok_or(ProjectionError::Closed)?
            .process_exited()
            .map_err(|_| ProjectionError::Capacity)?;
        state.exited = true;
        Ok(())
    }
    pub(super) fn finish_program_drain(&self) -> Result<(), ProjectionError> {
        let mut state = self.lock_program_status();
        if state.exited && !state.drained {
            state
                .store
                .as_mut()
                .ok_or(ProjectionError::Closed)?
                .process_exited()
                .map_err(|_| ProjectionError::Capacity)?;
            state.drained = true;
        }
        Ok(())
    }
    pub(super) fn clear_program_status(&self) {
        let mut state = self.lock_program_status();
        state.store = None;
        state.memory = None;
    }
}
