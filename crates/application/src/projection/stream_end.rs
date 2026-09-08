use super::{ProjectionCoordinator, ProjectionError, Residency, state::Engine};
use pty_runtime_domain::process::DrainOutcome;
impl ProjectionCoordinator {
    /// Record final reader drain independently of process exit or parser catchup.
    /// Existing queued work remains owned; later output/resizes are rejected. Safe
    /// before process binding. Readers invoke this after their final output callback.
    pub fn notify_output_drained(&self, outcome: DrainOutcome) {
        let failure = {
            let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
            if core.output_drain.is_none() {
                core.output_drain = Some(outcome);
            }
            core.policy.status().failure
        };
        if let Some(failure) = failure {
            self.journal.end(Some(outcome), Some(failure));
        }
        if self.wake().is_err() && self.status().residency != Residency::Closed {
            self.fail(ProjectionError::Worker);
        }
    }
    pub(super) fn finish_stream(&self, engine: &Engine) {
        let core = self.core.lock().unwrap_or_else(|e| e.into_inner());
        let status = core.policy.status();
        let drain = core.output_drain;
        let ready = drain.is_some()
            && !matches!(status.residency, Residency::Restoring | Residency::Usable)
            && !core.queue.iter().any(|event| {
                matches!(
                    event,
                    super::state::Event::Output(..) | super::state::Event::Resize(..)
                )
            })
            && engine.resize.is_none()
            && engine.reply.is_none()
            && status.processed == status.published
            && status.failure.is_none();
        drop(core);
        if ready {
            self.journal.end(drain, None);
        }
    }
}
