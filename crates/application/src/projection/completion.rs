use super::{
    PinnedCheckpoint, ProjectionCoordinator, ProjectionError, Residency,
    state::{
        BlockingJob, CommitOutcome, CommittedSource, IoResult, NativeWorkspace, UnreclaimedSource,
    },
};
use pty_runtime_domain::terminal::RestorationProgress;
impl ProjectionCoordinator {
    pub(super) fn finish_io(&self, workspace: &mut NativeWorkspace) {
        let Ok(services) = self.services() else {
            return;
        };
        let result = workspace.io.as_ref().and_then(|pending| {
            pending
                .mailbox
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take()
        });
        let Some(result) = result else { return };
        let Some(pending) = workspace.io.take() else {
            return;
        };
        let closing = matches!(
            self.status().residency,
            Residency::Closing | Residency::Closed
        );
        match pending.kind {
            BlockingJob::Commit {
                attempt,
                disk,
                _memory: _,
            } => {
                let result = match result {
                    IoResult::Commit(result) => result,
                    _ => CommitOutcome::Uncertain(ProjectionError::Worker),
                };
                match result {
                    CommitOutcome::Published(reference) => {
                        let source = CommittedSource {
                            reference,
                            descriptor: pty_runtime_domain::terminal::CheckpointDescriptor {
                                compatibility: services.terminal.compatibility().into(),
                                processed: attempt.processed,
                                control_generation: attempt.control_generation,
                            },
                            _disk: disk,
                        };
                        let release = {
                            let mut admission =
                                self.admission.lock().unwrap_or_else(|e| e.into_inner());
                            let empty = admission.queue.is_empty()
                                && workspace.reply.is_none()
                                && workspace.resize.is_none();
                            admission.policy.commit_park(attempt, empty)
                        };
                        if release {
                            // Logical release was atomic with the empty/activity check.
                            // Drop native state outside the aggregate lock; new output
                            // observes Parked and requests restoration on the next run.
                            workspace.terminal = None;
                            workspace.resident = None;
                            workspace.source = Some(source);
                        } else {
                            workspace.pending_deletes.push_back((source, 0));
                        }
                    }
                    CommitOutcome::Uncertain(error) => {
                        let entry = UnreclaimedSource {
                            _reference: None,
                            _key: pty_runtime_domain::checkpoint::CheckpointKey {
                                lifetime: attempt.processed.lifetime,
                                generation: attempt.generation,
                            },
                            _descriptor: pty_runtime_domain::terminal::CheckpointDescriptor {
                                compatibility: services.terminal.compatibility().into(),
                                processed: attempt.processed,
                                control_generation: attempt.control_generation,
                            },
                            _disk: disk,
                        };
                        self.budgets
                            .unreclaimed
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .push(entry);
                        let mut admission =
                            self.admission.lock().unwrap_or_else(|e| e.into_inner());
                        admission.unreclaimed_failure = Some(error);
                        admission.policy.park_failed(error, services.clock.now());
                    }
                    CommitOutcome::Rejected(error) => self
                        .admission
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .policy
                        .park_failed(error, services.clock.now()),
                }
            }
            BlockingJob::Restore { resident, memory } => {
                let checkpoint = match result {
                    IoResult::Read(result) => result,
                    _ => Err(ProjectionError::Worker),
                };
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
            BlockingJob::Transfer {
                request,
                memory,
                _staging: _,
            } => {
                let result = match result {
                    IoResult::Read(result) => result,
                    _ => Err(ProjectionError::Worker),
                };
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
            BlockingJob::Delete { source, attempts } => {
                let result = match result {
                    IoResult::Delete(result) => result,
                    _ => Err(ProjectionError::Worker),
                };
                if let Err(error) = result {
                    workspace.pending_deletes.push_front((source, attempts + 1));
                    if attempts + 1 >= self.options.max_park_attempts {
                        let mut admission =
                            self.admission.lock().unwrap_or_else(|e| e.into_inner());
                        admission.cleanup_failure = Some(error);
                        admission.policy.maintenance_failed(error);
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
        let result = self
            .admission
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .policy
            .restoration_progress(progress);
        if let Err(error) = result {
            self.fail(error);
            return;
        }
        if progress.is_finished() {
            workspace.restore_memory = None;
            if let Some(source) = workspace.source.take() {
                workspace.pending_deletes.push_back((source, 0));
            }
        }
    }
}
