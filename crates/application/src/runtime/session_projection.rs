use super::{RuntimeError, Session};
use crate::projection::{
    PinnedCheckpoint, ProjectedView, ProjectionCoordinator, ProjectionOperation,
};
use pty_runtime_domain::{
    projection::{ProjectionError, ProjectionStatus, ResizeOutcome},
    terminal::{TerminalError, TerminalSize},
};
use std::sync::Arc;

impl Session {
    /// Independent native residency and exact published/processed positions; None for raw mode.
    pub fn projection_status(&self) -> Result<Option<ProjectionStatus>, RuntimeError> {
        Ok(self
            .context
            .projection()?
            .map(|projection| projection.status()))
    }
    /// Admit a bounded copy of active terminal state, restoring a parked model as needed.
    /// Dropping the wait cancels observation only; returned buffers retain their quotas.
    pub fn projected_view(&self) -> Result<ProjectionOperation<ProjectedView>, RuntimeError> {
        Ok(self.projection()?.view()?)
    }
    /// Admit a compatible binary checkpoint at an exact processed byte/control boundary.
    /// A parked immutable source can satisfy this without waking its native model.
    /// The returned pin holds its buffer budget until dropped; no plaintext is persisted.
    pub fn terminal_checkpoint(
        &self,
    ) -> Result<ProjectionOperation<PinnedCheckpoint>, RuntimeError> {
        Ok(self.projection()?.checkpoint()?)
    }
    /// Order resize with parser output and report OS/model results separately.
    /// Use this in projected mode; raw sessions use `resize`. Dropping the wait does
    /// not undo an admitted control or hide a partial OS/model outcome.
    pub fn resize_projected(
        &self,
        size: TerminalSize,
    ) -> Result<ProjectionOperation<ResizeOutcome>, RuntimeError> {
        Ok(self.projection()?.resize(size)?)
    }
    fn projection(&self) -> Result<Arc<ProjectionCoordinator>, RuntimeError> {
        self.context
            .projection()?
            .ok_or_else(|| ProjectionError::Terminal(TerminalError::Unsupported).into())
    }
}
