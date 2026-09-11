use super::{
    PinnedCheckpoint, ProjectedView, ProjectionCoordinator, ProjectionError, ProjectionOperation,
    Residency, ResizeOutcome,
    budgets::StagingLease,
    observation::Ticket,
    queue::{CloseRequest, OutputAdmission, OutputOutcome},
    state::Command,
};
use crate::process::OutputAcceptance;
use pty_runtime_domain::terminal::TerminalSize;
use std::sync::{Arc, atomic::Ordering};
impl ProjectionCoordinator {
    pub(super) fn reserve_staging(&self, bytes: usize) -> Result<StagingLease, ProjectionError> {
        // Zero-payload controls already hold a bounded request ticket. Output
        // must not consume their admission; both remain in the same FIFO queue.
        let (slots, bytes) = if bytes == 0 {
            (None, None)
        } else {
            (
                Some(self.quotas.staging_slot()?),
                Some(self.quotas.staging_bytes(bytes)?),
            )
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
        Ok(Ticket::new(Some(self.quotas.request_slot()?)))
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
        let admitted = self.queue.admit_output(|admission| {
            if matches!(
                admission.residency(),
                Residency::Closing | Residency::Closed
            ) {
                return OutputAdmission::Rejected(OutputAcceptance::Closed);
            }
            if admission.is_draining() {
                return OutputAdmission::Rejected(OutputAcceptance::Closed);
            }
            if bytes.is_empty() {
                return OutputAdmission::Nothing;
            }
            if bytes.len() > self.config.options.terminal.feed_bytes {
                return OutputAdmission::Rejected(OutputAcceptance::Backpressure);
            }
            let Ok(mut lease) = self.reserve_staging(bytes.len()) else {
                return OutputAdmission::Rejected(OutputAcceptance::Backpressure);
            };
            let mut owned = Vec::new();
            if owned.try_reserve_exact(bytes.len()).is_err() {
                return OutputAdmission::Rejected(OutputAcceptance::Backpressure);
            }
            owned.extend_from_slice(bytes);
            if admission
                .admit_output_bytes(bytes.len(), services.clock.now())
                .is_err()
            {
                return OutputAdmission::Rejected(OutputAcceptance::Closed);
            }
            lease.timing = observed.map(|(diagnostics, started)| {
                crate::diagnostics::Timing::new(
                    diagnostics,
                    crate::diagnostics::LatencyKind::ProjectedOutput,
                    started,
                )
            });
            OutputAdmission::Queue(Command::Output(owned, lease))
        });
        match admitted {
            OutputOutcome::Rejected(rejection) => return rejection,
            // Nothing was queued, so there is nothing for the worker to do.
            OutputOutcome::Accepted => return OutputAcceptance::Accepted,
            OutputOutcome::Queued => (),
        }
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
        self.admit(|admission| {
            if admission.is_draining() {
                return Err(ProjectionError::Closed);
            }
            admission.record_activity(services.clock.now())?;
            lease.timing = timing;
            Ok(Command::Resize(size, ticket, lease))
        })?;
        Ok(wait)
    }
    /// Queue a bounded copied observation. Restores a parked model; complete history
    /// remains an explicit residency fact rather than being inferred from visible cells.
    pub fn view(&self) -> Result<ProjectionOperation<ProjectedView>, ProjectionError> {
        let (ticket, wait) = self.reserve_request_slot()?;
        let lease = self.reserve_staging(0)?;
        self.admit(|_| Ok(Command::View(ticket, lease)))?;
        Ok(wait)
    }
    /// Queue an immutable binary transfer pin at an exact byte/control boundary.
    /// A parked model supplies its saved source without restoring a native owner.
    pub fn checkpoint(&self) -> Result<ProjectionOperation<PinnedCheckpoint>, ProjectionError> {
        let (ticket, wait) = self.reserve_request_slot()?;
        let lease = self.reserve_staging(0)?;
        self.admit(|_| {
            Ok(Command::Checkpoint(
                super::snapshot::SnapshotRequest::Checkpoint(ticket),
                lease,
            ))
        })?;
        Ok(wait)
    }
    /// Invalidate pending publication and schedule source cleanup. The runtime retains
    /// this coordinator until the returned wait resolves. If wait admission is full,
    /// closure is still requested and the caller receives Capacity; inspect status.
    pub fn close(&self) -> Result<ProjectionOperation<()>, ProjectionError> {
        let pair = self.reserve_request_slot();
        let waiter = pair.as_ref().ok().map(|(ticket, _)| ticket);
        match self.queue.request_close(waiter) {
            CloseRequest::AlreadyClosed(outcome) => {
                if let Ok((ticket, _)) = &pair {
                    ticket.complete(outcome);
                }
            }
            CloseRequest::Started(rejected) => {
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
        }
        pair.map(|(_, wait)| wait)
    }
    /// Admit one command into the ordered queue and wake the worker.
    ///
    /// `build` runs under the admission lock, so it can apply any additional
    /// per-command checks and policy updates atomically with the push. Returning
    /// an error from it leaves the queue untouched and releases the guard, which
    /// drops whatever leases the caller had reserved.
    ///
    /// The guard is deliberately owned and dropped *here* rather than by the
    /// caller: `fail` re-acquires the admission lock, so waking while still
    /// holding it would deadlock. Keeping that ordering inside this one function
    /// means no call site can get it wrong.
    ///
    /// Once the push succeeds the command owns its leases, so a scheduler
    /// failure fails the projection instead of rolling the admission back.
    pub(super) fn admit(
        &self,
        build: impl FnOnce(&mut super::queue::Admitting<'_>) -> Result<Command, ProjectionError>,
    ) -> Result<(), ProjectionError> {
        self.queue.admit(build)?;
        if let Err(error) = self.wake() {
            self.fail(error);
        }
        Ok(())
    }
}
