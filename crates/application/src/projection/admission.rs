use super::{
    PinnedCheckpoint, ProjectedView, ProjectionCoordinator, ProjectionError, ProjectionOperation,
    Residency, ResizeOutcome,
    budgets::{Lease, StagingLease},
    observation::Ticket,
    state::Event,
};
use crate::process::OutputAcceptance;
use pty_runtime_domain::terminal::TerminalSize;
use std::sync::{Arc, atomic::Ordering};
impl ProjectionCoordinator {
    pub(super) fn staging(&self, bytes: usize) -> Result<StagingLease, ProjectionError> {
        let slots = Lease::pair(
            self.budgets.staging_slots.clone(),
            self.local_slots.clone(),
            1,
        )?;
        let bytes = if bytes == 0 {
            None
        } else {
            Some(Lease::pair(
                self.budgets.staging_bytes.clone(),
                self.local_bytes.clone(),
                bytes,
            )?)
        };
        Ok(StagingLease {
            bytes,
            slots: Some(slots),
            signal: self.services()?.capacity,
        })
    }
    fn ticket<T: Send + 'static>(
        &self,
    ) -> Result<(Arc<Ticket<T>>, ProjectionOperation<T>), ProjectionError> {
        let lease = Lease::pair(
            self.budgets.requests.clone(),
            self.local_requests.clone(),
            1,
        )?;
        Ok(Ticket::new(Some(lease)))
    }
    /// Admit an entire reader chunk once. Rejection neither copies nor advances positions.
    /// Replay publication must follow Accepted; evictable replay never backs this queue.
    pub fn stage_output(&self, bytes: &[u8]) -> OutputAcceptance {
        let Ok(services) = self.services() else {
            return OutputAcceptance::Closed;
        };
        let generation = services.capacity.generation();
        self.stall_generation.store(generation, Ordering::Release);
        let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
        if matches!(
            core.policy.status().residency,
            Residency::Closing | Residency::Closed
        ) {
            return OutputAcceptance::Closed;
        }
        if bytes.is_empty() {
            return OutputAcceptance::Accepted;
        }
        if bytes.len() > self.options.terminal.feed_bytes {
            return OutputAcceptance::Backpressure;
        }
        let Ok(lease) = self.staging(bytes.len()) else {
            return OutputAcceptance::Backpressure;
        };
        let mut owned = Vec::new();
        if owned.try_reserve_exact(bytes.len()).is_err() {
            return OutputAcceptance::Backpressure;
        }
        owned.extend_from_slice(bytes);
        if core
            .policy
            .admit_output(bytes.len(), services.clock.now())
            .is_err()
        {
            return OutputAcceptance::Closed;
        }
        core.queue.push_back(Event::Output(owned, lease));
        drop(core);
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
        let services = self.services()?;
        let (ticket, wait) = self.ticket()?;
        let lease = self.staging(0)?;
        let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
        self.accepting(&core)?;
        core.policy.activity(services.clock.now())?;
        core.queue.push_back(Event::Resize(size, ticket, lease));
        drop(core);
        if let Err(error) = self.wake() {
            self.fail(error);
        }
        Ok(wait)
    }
    /// Queue a bounded copied observation. Restores a parked model; complete history
    /// remains an explicit residency fact rather than being inferred from visible cells.
    pub fn view(&self) -> Result<ProjectionOperation<ProjectedView>, ProjectionError> {
        let (ticket, wait) = self.ticket()?;
        let lease = self.staging(0)?;
        let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
        self.accepting(&core)?;
        core.queue.push_back(Event::View(ticket, lease));
        drop(core);
        if let Err(error) = self.wake() {
            self.fail(error);
        }
        Ok(wait)
    }
    /// Queue an immutable binary transfer pin at an exact byte/control boundary.
    /// A parked model supplies its saved source without restoring a native owner.
    pub fn checkpoint(&self) -> Result<ProjectionOperation<PinnedCheckpoint>, ProjectionError> {
        let (ticket, wait) = self.ticket()?;
        let lease = self.staging(0)?;
        let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
        self.accepting(&core)?;
        core.queue.push_back(Event::Checkpoint(ticket, lease));
        drop(core);
        if let Err(error) = self.wake() {
            self.fail(error);
        }
        Ok(wait)
    }
    /// Invalidate pending publication and schedule source cleanup. The runtime retains
    /// this coordinator until the returned wait resolves. If wait admission is full,
    /// closure is still requested and the caller receives Capacity; inspect status.
    pub fn close(&self) -> Result<ProjectionOperation<()>, ProjectionError> {
        let pair = self.ticket();
        let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
        if core.policy.status().residency == Residency::Closed {
            let outcome = core.cleanup_failure.map_or(Ok(()), Err);
            drop(core);
            if let Ok((ticket, _)) = &pair {
                ticket.complete(outcome);
            }
        } else {
            core.policy.close();
            core.cleanup_failure = None;
            core.retry_cleanup = true;
            if let Ok((ticket, _)) = &pair {
                core.close_waiters.push(ticket.clone());
            }
            let rejected = std::mem::take(&mut core.queue);
            drop(core);
            for event in rejected {
                event.fail(ProjectionError::Closed);
            }
            if let Ok(services) = self.services() {
                services.capacity.notify();
            }
            self.wake()?;
        }
        pair.map(|(_, wait)| wait)
    }
    fn accepting(&self, core: &super::state::Core) -> Result<(), ProjectionError> {
        let status = core.policy.status();
        if matches!(status.residency, Residency::Closing | Residency::Closed) {
            return Err(ProjectionError::Closed);
        }
        if let Some(error) = status.failure {
            return Err(error);
        }
        Ok(())
    }
}
