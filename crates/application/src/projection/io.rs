//! The blocking-I/O lifecycle: starting one job and applying its result.
//!
//! The jobs themselves are in [`super::blocking`], the slot they land in is
//! [`super::inflight::InFlight`], and the deletion cycle belongs to
//! [`super::reaper::SourceReaper`]. What is left here is the part that could not
//! be given a type of its own: starting a read or a park, and applying whatever
//! comes back.
//!
//! These stayed `ProjectionCoordinator` methods deliberately, not by omission.
//! Each one acquires from two runtime-shared quotas, asks domain policy whether
//! the transition is permitted, drives the native engine and submits — in an
//! order where every step can fail and each failure has to put back exactly what
//! the step before it took. `finish_io` is that shape in reverse across four job
//! kinds. The measurement, and why a context object would only rename `self`,
//! are recorded in `docs/todo/declined.md`.
//!
//! Start and finish live in one file because they are one responsibility: every
//! `PendingIo` variant constructed below is destructured again in `finish_io`,
//! and the two halves have to agree about what each variant owns.
use super::{
    PinnedCheckpoint, ProjectionCoordinator, ProjectionError, Residency, blocking,
    budgets::{DiskLease, IoMemory, Lease},
    inflight::{FinishedIo, PendingIo},
    queue::ParkStart,
    state::{Command, CommitOutcome, CommittedSource, NativeWorkspace, UnreclaimedSource},
};
use crate::scheduling::WorkSchedule;
use pty_runtime_domain::{checkpoint::CheckpointKey, terminal::RestorationProgress};
use std::time::Duration;
/// How long to wait before retrying work a full pool or a full quota refused.
const RETRY_SOON: Duration = Duration::from_millis(5);
impl ProjectionCoordinator {
    pub(super) fn start_read(
        &self,
        workspace: &mut NativeWorkspace,
        transfer: Option<Command>,
    ) -> WorkSchedule {
        let Ok(services) = self.services() else {
            return WorkSchedule::Finished;
        };
        let Some(source) = &workspace.source else {
            self.fail(ProjectionError::Worker);
            return WorkSchedule::Dormant;
        };
        let memory = match IoMemory::acquire(
            &self.quotas.shared,
            workspace.config.checkpoint_bytes,
            self.config.protected_bytes,
        ) {
            Ok(memory) => memory,
            Err(error) => {
                if let Some(event) = transfer {
                    event.fail(error);
                }
                return WorkSchedule::After(RETRY_SOON);
            }
        };
        let job = blocking::read_job(
            services.store.clone(),
            services.protector.clone(),
            source.reference,
            source.descriptor.clone(),
            self.config.protected_bytes,
            workspace.config.checkpoint_bytes,
        );
        // Both destinations read the same source through the same job; they
        // differ only in what they own on completion and how they report a
        // rejected submission.
        if let Some(Command::Checkpoint(request, staging)) = transfer {
            match workspace.io.submit(
                services.blocking.as_ref(),
                self.wiring.handle(),
                job,
                (request, memory, staging),
                |(request, memory, _staging), mailbox| PendingIo::Transfer {
                    request,
                    memory,
                    _staging,
                    mailbox,
                },
            ) {
                Ok(()) => WorkSchedule::Dormant,
                Err(((request, ..), error)) => {
                    request.fail(error);
                    WorkSchedule::After(RETRY_SOON)
                }
            }
        } else {
            let resident = match Lease::shared(
                self.quotas.shared.resident.clone(),
                workspace.config.native_bytes,
            ) {
                Ok(resident) => resident,
                Err(_) => return WorkSchedule::After(RETRY_SOON),
            };
            match workspace.io.submit(
                services.blocking.as_ref(),
                self.wiring.handle(),
                job,
                (resident, memory),
                |(resident, memory), mailbox| PendingIo::Restore {
                    resident,
                    memory,
                    mailbox,
                },
            ) {
                Ok(()) => {
                    let result = self.queue.begin_restore();
                    if let Err(error) = result {
                        self.fail(error);
                    }
                    WorkSchedule::Dormant
                }
                Err(_) => WorkSchedule::After(RETRY_SOON),
            }
        }
    }
    pub(super) fn start_park(&self, workspace: &mut NativeWorkspace) -> WorkSchedule {
        let Ok(services) = self.services() else {
            return WorkSchedule::Finished;
        };
        let attempt = match self.queue.begin_park(services.clock.now()) {
            ParkStart::Ready(attempt) => attempt,
            ParkStart::Busy => return WorkSchedule::After(Duration::ZERO),
            ParkStart::NotDue => return WorkSchedule::Dormant,
        };
        let result = (|| {
            let max = self.config.protected_bytes;
            let memory = IoMemory::acquire(
                &self.quotas.shared,
                workspace.config.checkpoint_bytes,
                self.config.protected_bytes,
            )?;
            let disk = DiskLease::acquire(&self.quotas.shared, max)?;
            let terminal = workspace.terminal.as_mut().ok_or(ProjectionError::Closed)?;
            let descriptor = self.descriptor();
            let checkpoint = self.native_call(|| terminal.checkpoint(descriptor.clone()))?;
            if checkpoint.descriptor != descriptor {
                return Err(ProjectionError::InvalidConfiguration);
            }
            if checkpoint.bytes.capacity() > workspace.config.checkpoint_bytes {
                return Err(ProjectionError::Capacity);
            }
            Ok((checkpoint, memory, disk))
        })();
        let (checkpoint, memory, disk) = match result {
            Ok(values) => values,
            Err(error) => {
                self.queue.park_failed(error, services.clock.now());
                return WorkSchedule::After(self.config.options.retry_after);
            }
        };
        let descriptor = checkpoint.descriptor.clone();
        let job = blocking::commit_job(
            services.store.clone(),
            services.protector.clone(),
            CheckpointKey {
                lifetime: attempt.processed.lifetime,
                generation: attempt.generation,
            },
            checkpoint,
            descriptor,
            self.config.protected_bytes,
        );
        match workspace.io.submit(
            services.blocking.as_ref(),
            self.wiring.handle(),
            job,
            (attempt, disk, memory),
            |(attempt, disk, _memory), mailbox| PendingIo::Commit {
                attempt,
                disk,
                _memory,
                mailbox,
            },
        ) {
            Ok(()) => WorkSchedule::Dormant,
            Err((_, error)) => {
                self.queue.park_failed(error, services.clock.now());
                WorkSchedule::After(self.config.options.retry_after)
            }
        }
    }

    /// Apply the result of the one blocking job, if it has finished.
    ///
    /// Each arm reads its own typed mailbox, so a result can only ever be
    /// interpreted as the kind of work that produced it. An empty or poisoned
    /// mailbox is worker failure and nothing else.
    pub(super) fn finish_io(&self, workspace: &mut NativeWorkspace) {
        let Ok(services) = self.services() else {
            return;
        };
        let Some(pending) = workspace.io.take_finished() else {
            return;
        };
        let closing = matches!(
            self.status().residency,
            Residency::Closing | Residency::Closed
        );
        match pending {
            FinishedIo::Commit {
                attempt,
                disk,
                _memory: _,
                result,
            } => {
                // A panicked commit worker may have stored ciphertext before it
                // died, so it is Uncertain rather than Rejected.
                match result.unwrap_or_else(CommitOutcome::Uncertain) {
                    CommitOutcome::Published(reference) => {
                        let source = CommittedSource {
                            reference,
                            descriptor: pty_runtime_domain::terminal::CheckpointDescriptor {
                                compatibility: self.config.compatibility.clone(),
                                processed: attempt.processed,
                                control_generation: attempt.control_generation,
                            },
                            _disk: disk,
                        };
                        let release = self.queue.commit_park(attempt);
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
                            &self.config.compatibility,
                            attempt,
                            disk,
                        );
                        self.quotas.shared.record_unreclaimed(entry);
                        self.queue
                            .park_outcome_uncertain(error, services.clock.now());
                    }
                    CommitOutcome::Rejected(error) => {
                        self.queue.park_failed(error, services.clock.now())
                    }
                }
            }
            FinishedIo::Restore {
                resident,
                memory,
                result: checkpoint,
            } => {
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
            FinishedIo::Transfer {
                request,
                memory,
                _staging: _,
                result,
            } => {
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
            FinishedIo::Delete { attempt, result } => {
                if let Some(error) = workspace.reaper.finish_delete(attempt, result) {
                    self.queue.maintenance_failed(error);
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
