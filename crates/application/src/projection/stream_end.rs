use super::{ProjectionCoordinator, ProjectionError, Residency, state::NativeWorkspace};
use pty_runtime_domain::process::DrainOutcome;
impl ProjectionCoordinator {
    /// Record final reader drain independently of process exit or parser catchup.
    /// Existing queued work remains owned; later output/resizes are rejected. Safe
    /// before process binding. Readers invoke this after their final output callback.
    pub fn notify_output_drained(&self, outcome: DrainOutcome) {
        if let Some(failure) = self.queue.record_drain(outcome) {
            self.journal.end(Some(outcome), Some(failure));
        }
        if self.wake().is_err() && self.status().residency != Residency::Closed {
            self.fail(ProjectionError::Worker);
        }
    }
    /// Seal the continuation stream once every admitted mutation has been
    /// applied and nothing can still extend it.
    pub(super) fn seal_journal_if_drained(&self, workspace: &NativeWorkspace) {
        let engine_idle = workspace.resize.is_none() && workspace.reply.is_none();
        if let Some(drain) = self.queue.settled_drain(engine_idle) {
            self.journal.end(Some(drain), None);
        }
    }
}
