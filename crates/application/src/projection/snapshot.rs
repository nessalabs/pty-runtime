use super::{
    PinnedCheckpoint, ProjectionCoordinator, ProjectionError, StateTransfer, budgets::Lease,
    journal::Journal, observation::Ticket, state::Engine,
};
use std::sync::Arc;

pub(super) enum SnapshotRequest {
    Checkpoint(Arc<Ticket<PinnedCheckpoint>>),
    Transfer(Arc<Ticket<StateTransfer>>, Lease),
}
impl SnapshotRequest {
    pub fn cancelled(&self) -> bool {
        match self {
            Self::Checkpoint(ticket) => ticket.cancelled(),
            Self::Transfer(ticket, _) => ticket.cancelled(),
        }
    }
    pub fn fail(self, error: ProjectionError) {
        match self {
            Self::Checkpoint(ticket) => ticket.complete(Err(error)),
            Self::Transfer(ticket, _) => ticket.complete(Err(error)),
        }
    }
    pub fn complete(
        self,
        result: Result<PinnedCheckpoint, ProjectionError>,
        journal: &Arc<Journal>,
    ) {
        match self {
            Self::Checkpoint(ticket) => ticket.complete(result),
            Self::Transfer(ticket, permit) => {
                if ticket.cancelled() {
                    return;
                }
                ticket.complete(result.and_then(|checkpoint| {
                    let observer = journal.open(permit)?;
                    let boundary = observer.boundary();
                    if boundary.processed != checkpoint.checkpoint().descriptor.processed
                        || boundary.control_generation
                            != checkpoint.checkpoint().descriptor.control_generation
                    {
                        return Err(ProjectionError::InvalidConfiguration);
                    }
                    Ok(StateTransfer {
                        checkpoint,
                        observer,
                    })
                }));
            }
        }
    }
}
impl ProjectionCoordinator {
    pub(super) fn snapshot(&self, engine: &mut Engine, request: SnapshotRequest) {
        if request.cancelled() {
            return;
        }
        let result = Lease::one(
            self.budgets.checkpoints.clone(),
            engine.config.checkpoint_bytes,
        )
        .and_then(|lease| {
            let descriptor = self.descriptor();
            let terminal = engine.terminal.as_mut().ok_or(ProjectionError::Closed)?;
            let checkpoint = self.native_call(|| terminal.checkpoint(descriptor.clone()))?;
            if checkpoint.descriptor != descriptor {
                return Err(ProjectionError::InvalidConfiguration);
            }
            if checkpoint.bytes.capacity() > engine.config.checkpoint_bytes {
                return Err(ProjectionError::Capacity);
            }
            Ok(PinnedCheckpoint {
                checkpoint,
                _lease: lease,
            })
        });
        if matches!(
            self.status().residency,
            super::Residency::Closing | super::Residency::Closed
        ) {
            request.fail(ProjectionError::Closed);
        } else {
            request.complete(result, &self.journal);
        }
    }
}

impl ProjectionCoordinator {
    /// Admit a bounded checkpoint plus independent ordered continuation observer.
    /// Cancelling the wait releases provisional observer admission. A parked source
    /// supplies the snapshot without native restoration; staged mutations follow it.
    pub fn begin_transfer(
        &self,
    ) -> Result<super::ProjectionOperation<StateTransfer>, ProjectionError> {
        let permit = self.journal.reserve_observer()?;
        let (ticket, wait) = self.ticket()?;
        let staging = self.staging(0)?;
        let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
        self.accepting(&core)?;
        core.queue.push_back(super::state::Event::Checkpoint(
            SnapshotRequest::Transfer(ticket, permit),
            staging,
        ));
        drop(core);
        if let Err(error) = self.wake() {
            self.fail(error);
        }
        Ok(wait)
    }
}
