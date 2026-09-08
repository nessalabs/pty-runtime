use super::{
    PinnedCheckpoint, ProjectionCoordinator, ProjectionError, Residency,
    state::{CommitOutcome, Engine, IoKind, IoResult, Stored, Unreclaimed},
};
use pty_runtime_domain::terminal::RestorationProgress;
impl ProjectionCoordinator {
    pub(super) fn finish_io(&self, engine: &mut Engine) {
        let Ok(services) = self.services() else {
            return;
        };
        let result = engine.io.as_ref().and_then(|pending| {
            pending
                .mailbox
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take()
        });
        let Some(result) = result else { return };
        let Some(pending) = engine.io.take() else {
            return;
        };
        let closing = matches!(
            self.status().residency,
            Residency::Closing | Residency::Closed
        );
        match pending.kind {
            IoKind::Commit {
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
                        let source = Stored {
                            reference,
                            descriptor: pty_runtime_domain::terminal::CheckpointDescriptor {
                                compatibility: services.terminal.compatibility().into(),
                                processed: attempt.processed,
                                control_generation: attempt.control_generation,
                            },
                            _disk: disk,
                        };
                        let release = {
                            let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
                            let empty = core.queue.is_empty()
                                && engine.reply.is_none()
                                && engine.resize.is_none();
                            core.policy.commit_park(attempt, empty)
                        };
                        if release {
                            // Logical release was atomic with the empty/activity check.
                            // Drop native state outside the aggregate lock; new output
                            // observes Parked and requests restoration on the next run.
                            engine.terminal = None;
                            engine.resident = None;
                            engine.source = Some(source);
                        } else {
                            engine.garbage.push_back((source, 0));
                        }
                    }
                    CommitOutcome::Uncertain(error) => {
                        let entry = Unreclaimed {
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
                        let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
                        core.unreclaimed_failure = Some(error);
                        core.policy.park_failed(error, services.clock.now());
                    }
                    CommitOutcome::Rejected(error) => self
                        .core
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .policy
                        .park_failed(error, services.clock.now()),
                }
            }
            IoKind::Restore { resident, memory } => {
                let checkpoint = match result {
                    IoResult::Read(result) => result,
                    _ => Err(ProjectionError::Worker),
                };
                if !closing {
                    match checkpoint.and_then(|checkpoint| {
                        self.native_call(|| services.terminal.restore(checkpoint, engine.config))
                    }) {
                        Ok(terminal) => {
                            let complete =
                                match self.native_call(|| Ok(terminal.restoration_progress())) {
                                    Ok(progress) => progress == RestorationProgress::Complete,
                                    Err(_) => return,
                                };
                            engine.terminal = Some(terminal);
                            engine.resident = Some(resident);
                            engine.restore_memory = Some(memory.plain);
                            if complete {
                                self.restored(engine);
                            } else {
                                let result = self
                                    .core
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner())
                                    .policy
                                    .restoration_progress(RestorationProgress::Usable);
                                if let Err(error) = result {
                                    self.fail(error);
                                }
                            }
                        }
                        Err(error) => self.fail(error),
                    }
                }
            }
            IoKind::Transfer {
                ticket,
                memory,
                _staging: _,
            } => {
                let result = match result {
                    IoResult::Read(result) => result,
                    _ => Err(ProjectionError::Worker),
                };
                ticket.complete(if closing {
                    Err(ProjectionError::Closed)
                } else {
                    result.map(|checkpoint| PinnedCheckpoint {
                        checkpoint,
                        _lease: memory.plain,
                    })
                });
            }
            IoKind::Delete { source, attempts } => {
                let result = match result {
                    IoResult::Delete(result) => result,
                    _ => Err(ProjectionError::Worker),
                };
                if let Err(error) = result {
                    engine.garbage.push_front((source, attempts + 1));
                    if attempts + 1 >= self.options.max_park_attempts {
                        let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
                        core.cleanup_failure = Some(error);
                        core.policy.maintenance_failed(error);
                    }
                }
            }
        }
    }
    pub(super) fn restored(&self, engine: &mut Engine) {
        engine.restore_memory = None;
        if let Some(source) = engine.source.take() {
            engine.garbage.push_back((source, 0));
        }
        let result = self
            .core
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .policy
            .restoration_progress(RestorationProgress::Complete);
        if let Err(error) = result {
            self.fail(error);
        }
    }
}
