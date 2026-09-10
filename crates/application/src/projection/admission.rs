use super::{
    PinnedCheckpoint, ProjectedView, ProjectionCoordinator, ProjectionError, ProjectionOperation,
    Residency, ResizeOutcome,
    budgets::{Lease, StagingLease},
    observation::Ticket,
    state::Command,
};
use crate::process::OutputAcceptance;
use pty_runtime_domain::terminal::TerminalSize;
use std::sync::{Arc, atomic::Ordering};
impl ProjectionCoordinator {
    pub(super) fn reserve_staging(&self, bytes: usize) -> Result<StagingLease, ProjectionError> {
        // Zero-payload controls already hold a bounded request ticket. Output
        // must not consume their admission; both remain in the same FIFO queue.
        let slots = if bytes == 0 {
            None
        } else {
            Some(Lease::shared_and_local(
                self.budgets.staging_slots.clone(),
                self.local_slots.clone(),
                1,
            )?)
        };
        let bytes = if bytes == 0 {
            None
        } else {
            Some(Lease::shared_and_local(
                self.budgets.staging_bytes.clone(),
                self.local_bytes.clone(),
                bytes,
            )?)
        };
        Ok(StagingLease {
            timing: None,
            bytes,
            slots,
            signal: self.services()?.capacity,
        })
    }
    pub(super) fn reserve_request_slot<T: Send + 'static>(
        &self,
    ) -> Result<(Arc<Ticket<T>>, ProjectionOperation<T>), ProjectionError> {
        let lease = Lease::shared_and_local(
            self.budgets.requests.clone(),
            self.local_requests.clone(),
            1,
        )?;
        Ok(Ticket::new(Some(lease)))
    }
    /// Admit an entire reader chunk once. Rejection neither copies nor advances positions.
    /// Replay publication must follow Accepted; evictable replay never backs this queue.
    pub fn stage_output(&self, bytes: &[u8]) -> OutputAcceptance {
        self.stage_output_observed(bytes, None)
    }
    /// Preserve an actual read-completion timestamp through accepted parser staging.
    /// Rejected chunks retain no timer and must be retried with the original timestamp.
    pub fn stage_output_observed(
        &self,
        bytes: &[u8],
        observed: Option<(
            Arc<crate::diagnostics::RuntimeDiagnostics>,
            std::time::Instant,
        )>,
    ) -> OutputAcceptance {
        let Ok(services) = self.services() else {
            return OutputAcceptance::Closed;
        };
        let generation = services.capacity.generation();
        self.stall_generation.store(generation, Ordering::Release);
        let mut admission = self.admission.lock().unwrap_or_else(|e| e.into_inner());
        if matches!(
            admission.policy.status().residency,
            Residency::Closing | Residency::Closed
        ) {
            return OutputAcceptance::Closed;
        }
        if admission.output_drain.is_some() {
            return OutputAcceptance::Closed;
        }
        if bytes.is_empty() {
            return OutputAcceptance::Accepted;
        }
        if bytes.len() > self.options.terminal.feed_bytes {
            return OutputAcceptance::Backpressure;
        }
        let Ok(mut lease) = self.reserve_staging(bytes.len()) else {
            return OutputAcceptance::Backpressure;
        };
        let mut owned = Vec::new();
        if owned.try_reserve_exact(bytes.len()).is_err() {
            return OutputAcceptance::Backpressure;
        }
        owned.extend_from_slice(bytes);
        if admission
            .policy
            .admit_output(bytes.len(), services.clock.now())
            .is_err()
        {
            return OutputAcceptance::Closed;
        }
        lease.timing = observed.map(|(diagnostics, started)| {
            crate::diagnostics::Timing::new(
                diagnostics,
                crate::diagnostics::LatencyKind::ProjectedOutput,
                started,
            )
        });
        admission.queue.push_back(Command::Output(owned, lease));
        drop(admission);
        // Accepted staging is never rolled back after ownership transfer, even if
        // scheduler failure is reported. Failure preserves bytes for diagnostics.
        if self.wake().is_err() {
            self.fail(ProjectionError::Worker);
        }
        OutputAcceptance::Accepted
    }
    /// Admit OS/model resize in the same ordered queue as reader output. Abandoning
    /// its returned wait does not cancel either side of the admitted control.
    pub fn resize(
        &self,
        size: TerminalSize,
    ) -> Result<ProjectionOperation<ResizeOutcome>, ProjectionError> {
        self.resize_timed(size, None)
    }
    /// Preserve optional resize admission timing through the ordered queue.
    pub fn resize_timed(
        &self,
        size: TerminalSize,
        timing: Option<crate::diagnostics::Timing>,
    ) -> Result<ProjectionOperation<ResizeOutcome>, ProjectionError> {
        let services = self.services()?;
        let (ticket, wait) = self.reserve_request_slot()?;
        let mut lease = self.reserve_staging(0)?;
        let mut admission = self.admission.lock().unwrap_or_else(|e| e.into_inner());
        self.ensure_accepting(&admission)?;
        if admission.output_drain.is_some() {
            return Err(ProjectionError::Closed);
        }
        admission.policy.record_activity(services.clock.now())?;
        lease.timing = timing;
        admission
            .queue
            .push_back(Command::Resize(size, ticket, lease));
        drop(admission);
        if let Err(error) = self.wake() {
            self.fail(error);
        }
        Ok(wait)
    }
    /// Queue a bounded copied observation. Restores a parked model; complete history
    /// remains an explicit residency fact rather than being inferred from visible cells.
    pub fn view(&self) -> Result<ProjectionOperation<ProjectedView>, ProjectionError> {
        let (ticket, wait) = self.reserve_request_slot()?;
        let lease = self.reserve_staging(0)?;
        let mut admission = self.admission.lock().unwrap_or_else(|e| e.into_inner());
        self.ensure_accepting(&admission)?;
        admission.queue.push_back(Command::View(ticket, lease));
        drop(admission);
        if let Err(error) = self.wake() {
            self.fail(error);
        }
        Ok(wait)
    }
    /// Queue an immutable binary transfer pin at an exact byte/control boundary.
    /// A parked model supplies its saved source without restoring a native owner.
    pub fn checkpoint(&self) -> Result<ProjectionOperation<PinnedCheckpoint>, ProjectionError> {
        let (ticket, wait) = self.reserve_request_slot()?;
        let lease = self.reserve_staging(0)?;
        let mut admission = self.admission.lock().unwrap_or_else(|e| e.into_inner());
        self.ensure_accepting(&admission)?;
        admission.queue.push_back(Command::Checkpoint(
            super::snapshot::SnapshotRequest::Checkpoint(ticket),
            lease,
        ));
        drop(admission);
        if let Err(error) = self.wake() {
            self.fail(error);
        }
        Ok(wait)
    }
    /// Invalidate pending publication and schedule source cleanup. The runtime retains
    /// this coordinator until the returned wait resolves. If wait admission is full,
    /// closure is still requested and the caller receives Capacity; inspect status.
    pub fn close(&self) -> Result<ProjectionOperation<()>, ProjectionError> {
        let pair = self.reserve_request_slot();
        let mut admission = self.admission.lock().unwrap_or_else(|e| e.into_inner());
        if admission.policy.status().residency == Residency::Closed {
            let outcome = admission.cleanup_failure.map_or(Ok(()), Err);
            drop(admission);
            if let Ok((ticket, _)) = &pair {
                ticket.complete(outcome);
            }
        } else {
            admission.policy.close();
            admission.cleanup_failure = None;
            admission.retry_cleanup = true;
            if let Ok((ticket, _)) = &pair {
                admission.close_waiters.push(ticket.clone());
            }
            let rejected = std::mem::take(&mut admission.queue);
            drop(admission);
            self.journal.close();
            for event in rejected {
                event.fail(ProjectionError::Closed);
            }
            if let Ok(services) = self.services() {
                services.capacity.notify();
            }
            // An already scheduled worker may finish closure and release its
            // handle before this wake. Preserve its durable cleanup result;
            // an unfinished close still reports a genuine scheduler failure.
            match self.wake() {
                Err(error) if self.close_outcome().is_none() => return Err(error),
                _ => (),
            }
        }
        pair.map(|(_, wait)| wait)
    }
    pub(super) fn ensure_accepting(
        &self,
        admission: &super::state::Admission,
    ) -> Result<(), ProjectionError> {
        let status = admission.policy.status();
        if matches!(status.residency, Residency::Closing | Residency::Closed) {
            return Err(ProjectionError::Closed);
        }
        if let Some(error) = status.failure {
            return Err(error);
        }
        Ok(())
    }
}
