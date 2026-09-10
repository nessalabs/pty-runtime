use super::{ProjectionCoordinator, ProjectionError, Residency, state::NativeWorkspace};
use pty_runtime_domain::process::DrainOutcome;
impl ProjectionCoordinator {
    /// Record final reader drain independently of process exit or parser catchup.
    /// Existing queued work remains owned; later output/resizes are rejected. Safe
    /// before process binding. Readers invoke this after their final output callback.
    pub fn notify_output_drained(&self, outcome: DrainOutcome) {
        let failure = {
            let mut admission = self.admission.lock().unwrap_or_else(|e| e.into_inner());
            if admission.output_drain.is_none() {
                admission.output_drain = Some(outcome);
            }
            admission.policy.status().failure
        };
        if let Some(failure) = failure {
            self.journal.end(Some(outcome), Some(failure));
        }
        if self.wake().is_err() && self.status().residency != Residency::Closed {
            self.fail(ProjectionError::Worker);
        }
    }
    pub(super) fn seal_journal_if_drained(&self, workspace: &NativeWorkspace) {
        let admission = self.admission.lock().unwrap_or_else(|e| e.into_inner());
        let status = admission.policy.status();
        let drain = admission.output_drain;
        let ready = drain.is_some()
            && !matches!(status.residency, Residency::Restoring | Residency::Usable)
            && !admission.queue.iter().any(|event| {
                matches!(
                    event,
                    super::state::Command::Output(..) | super::state::Command::Resize(..)
                )
            })
            && workspace.resize.is_none()
            && workspace.reply.is_none()
            && status.processed == status.published
            && status.failure.is_none();
        drop(admission);
        if ready {
            self.journal.end(drain, None);
        }
    }
}
