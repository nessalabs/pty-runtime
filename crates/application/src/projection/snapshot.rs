use super::{
    PinnedCheckpoint, ProjectionCoordinator, ProjectionError, StateTransfer, budgets::Lease,
    journal::Journal, observation::Ticket, state::NativeWorkspace,
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
    pub(super) fn snapshot(&self, workspace: &mut NativeWorkspace, request: SnapshotRequest) {
        if request.cancelled() {
            return;
        }
        let result = Lease::shared(
            self.budgets.checkpoints.clone(),
            workspace.config.checkpoint_bytes,
        )
        .and_then(|lease| {
            let descriptor = self.descriptor();
            let terminal = workspace.terminal.as_mut().ok_or(ProjectionError::Closed)?;
            let checkpoint = self.native_call(|| terminal.checkpoint(descriptor.clone()))?;
            if checkpoint.descriptor != descriptor {
                return Err(ProjectionError::InvalidConfiguration);
            }
            if checkpoint.bytes.capacity() > workspace.config.checkpoint_bytes {
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
        let (ticket, wait) = self.reserve_request_slot()?;
        let staging = self.reserve_staging(0)?;
        self.admit(|_| {
            Ok(super::state::Command::Checkpoint(
                SnapshotRequest::Transfer(ticket, permit),
                staging,
            ))
        })?;
        Ok(wait)
    }
}
