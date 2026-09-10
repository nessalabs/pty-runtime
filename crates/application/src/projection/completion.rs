use super::{
    PinnedCheckpoint, ProjectionCoordinator, ProjectionError, Residency,
    state::{
        CommitOutcome, CommittedSource, NativeWorkspace, PendingIo, UnreclaimedSource, collect,
    },
};
use pty_runtime_domain::terminal::RestorationProgress;
impl ProjectionCoordinator {
    /// Apply the result of the one blocking job, if it has finished.
    ///
    /// Each arm reads its own typed mailbox, so a result can only ever be
    /// interpreted as the kind of work that produced it. An empty or poisoned
    /// mailbox is worker failure and nothing else.
    pub(super) fn finish_io(&self, workspace: &mut NativeWorkspace) {
        let Ok(services) = self.services() else {
            return;
        };
        if !workspace.io.as_ref().is_some_and(PendingIo::is_ready) {
            return;
        }
        let Some(pending) = workspace.io.take() else {
            return;
        };
        let closing = matches!(
            self.status().residency,
            Residency::Closing | Residency::Closed
        );
        match pending {
            PendingIo::Commit {
                attempt,
                disk,
                _memory: _,
                mailbox,
            } => {
                // A panicked commit worker may have stored ciphertext before it
                // died, so it is Uncertain rather than Rejected.
                let result = collect(&mailbox).unwrap_or_else(CommitOutcome::Uncertain);
                match result {
                    CommitOutcome::Published(reference) => {
                        let source = CommittedSource {
                            reference,
                            descriptor: pty_runtime_domain::terminal::CheckpointDescriptor {
                                compatibility: self.wiring.compatibility.clone(),
                                processed: attempt.processed,
                                control_generation: attempt.control_generation,
                            },
                            _disk: disk,
                        };
                        let engine_idle = workspace.reply.is_none() && workspace.resize.is_none();
                        let release = self.queue.commit_park(attempt, engine_idle);
                        if release {
                            // Logical release was atomic with the empty/activity check.
                            // Drop native state outside the aggregate lock; new output
                            // observes Parked and requests restoration on the next run.
                            workspace.terminal = None;
                            workspace.resident = None;
                            workspace.source = Some(source);
                        } else {
                            workspace.reaper.retire(source);
                        }
                    }
                    CommitOutcome::Uncertain(error) => {
                        let entry = UnreclaimedSource::from_uncertain_park(
                            &self.wiring.compatibility,
                            attempt,
                            disk,
                        );
                        self.quotas
                            .shared
                            .unreclaimed
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .push(entry);
                        self.queue
                            .park_outcome_uncertain(error, services.clock.now());
                    }
                    CommitOutcome::Rejected(error) => {
                        self.queue.park_failed(error, services.clock.now())
                    }
                }
            }
            PendingIo::Restore {
                resident,
                memory,
                mailbox,
            } => {
                let checkpoint = collect(&mailbox);
                if !closing {
                    match checkpoint.and_then(|checkpoint| {
                        self.native_call(|| services.terminal.restore(checkpoint, workspace.config))
                    }) {
                        Ok(terminal) => {
                            let progress =
                                match self.native_call(|| Ok(terminal.restoration_progress())) {
                                    Ok(progress) => progress,
                                    Err(_) => return,
                                };
                            workspace.terminal = Some(terminal);
                            workspace.resident = Some(resident);
                            workspace.restore_memory = Some(memory.plain);
                            workspace.history_step_owed = false;
                            self.history_progress(workspace, progress);
                        }
                        Err(error) => self.fail(error),
                    }
                }
            }
            PendingIo::Transfer {
                request,
                memory,
                _staging: _,
                mailbox,
            } => {
                let result = collect(&mailbox);
                request.complete(
                    if closing {
                        Err(ProjectionError::Closed)
                    } else {
                        result.map(|checkpoint| PinnedCheckpoint {
                            checkpoint,
                            _lease: memory.plain,
                        })
                    },
                    &self.journal,
                );
            }
            PendingIo::Delete { attempt, mailbox } => {
                // A successful delete drops the source here, releasing its disk
                // reservation. A failure is only durable once retries are spent.
                if let Err(error) = collect(&mailbox) {
                    if let Some(error) = workspace.reaper.give_up_or_retry(attempt, error) {
                        self.queue.maintenance_failed(error);
                    }
                }
            }
        }
    }
    pub(super) fn history_progress(
        &self,
        workspace: &mut NativeWorkspace,
        progress: RestorationProgress,
    ) {
        let result = self.queue.record_restoration_progress(progress);
        if let Err(error) = result {
            self.fail(error);
            return;
        }
        if progress.is_finished() {
            workspace.restore_memory = None;
            if let Some(source) = workspace.source.take() {
                workspace.reaper.retire(source);
            }
        }
    }
}
