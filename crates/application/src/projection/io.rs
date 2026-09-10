use super::{
    ProjectionCoordinator, ProjectionError,
    budgets::{DiskLease, IoMemory, Lease},
    state::{Command, CommitOutcome, Mailbox, NativeWorkspace, PendingIo},
};
use crate::scheduling::WorkSchedule;
use pty_runtime_domain::checkpoint::CheckpointKey;
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex},
    time::Duration,
};
impl ProjectionCoordinator {
    /// Hand one job to the blocking pool and return the mailbox it will fill.
    ///
    /// The payload type is chosen by the caller and travels with the mailbox
    /// into [`PendingIo`], so completion cannot misread one job's result as
    /// another's. A panicking job publishes `Err(Worker)`.
    fn submit_io<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> Result<T, ProjectionError> + Send + 'static,
    ) -> Result<Mailbox<T>, ProjectionError> {
        let services = self.services()?;
        let mailbox: Mailbox<T> = Arc::new(Mutex::new(None));
        let output = mailbox.clone();
        let wake = self
            .handle
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let job = Box::new(move || {
            let result =
                catch_unwind(AssertUnwindSafe(work)).unwrap_or(Err(ProjectionError::Worker));
            *output.lock().unwrap_or_else(|e| e.into_inner()) = Some(result);
            if let Some(wake) = wake {
                let _ = wake.wake();
            }
        });
        if services.blocking.submit(job).is_err() {
            return Err(ProjectionError::Capacity);
        }
        Ok(mailbox)
    }
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
            &self.budgets,
            workspace.config.checkpoint_bytes,
            self.protected_bytes,
        ) {
            Ok(memory) => memory,
            Err(error) => {
                if let Some(event) = transfer {
                    event.fail(error);
                }
                return WorkSchedule::After(Duration::from_millis(5));
            }
        };
        let store = services.store.clone();
        let protector = services.protector.clone();
        let reference = source.reference;
        let descriptor = source.descriptor.clone();
        let max = self.protected_bytes;
        let plaintext_max = workspace.config.checkpoint_bytes;
        let job = move || {
            let bytes = store.read(reference, max)?;
            if bytes.ciphertext().len() != reference.bytes {
                return Err(ProjectionError::Storage(
                    pty_runtime_domain::checkpoint::CheckpointError::Unavailable,
                ));
            }
            let checkpoint = protector.open(reference.key, &descriptor, bytes)?;
            if checkpoint.descriptor != descriptor || checkpoint.bytes.capacity() > plaintext_max {
                return Err(ProjectionError::InvalidConfiguration);
            }
            Ok(checkpoint)
        };
        // Both destinations read the same source through the same job; they
        // differ only in what they own on completion and how they report a
        // rejected submission.
        if let Some(Command::Checkpoint(request, staging)) = transfer {
            match self.submit_io(job) {
                Ok(mailbox) => {
                    workspace.io = Some(PendingIo::Transfer {
                        request,
                        memory,
                        _staging: staging,
                        mailbox,
                    });
                    WorkSchedule::Dormant
                }
                Err(error) => {
                    request.fail(error);
                    WorkSchedule::After(Duration::from_millis(5))
                }
            }
        } else {
            let resident =
                match Lease::shared(self.budgets.resident.clone(), workspace.config.native_bytes) {
                    Ok(resident) => resident,
                    Err(_) => return WorkSchedule::After(Duration::from_millis(5)),
                };
            match self.submit_io(job) {
                Ok(mailbox) => {
                    let result = self
                        .admission
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .policy
                        .begin_restore();
                    if let Err(error) = result {
                        self.fail(error);
                    }
                    workspace.io = Some(PendingIo::Restore {
                        resident,
                        memory,
                        mailbox,
                    });
                    WorkSchedule::Dormant
                }
                Err(_) => WorkSchedule::After(Duration::from_millis(5)),
            }
        }
    }
    pub(super) fn start_park(&self, workspace: &mut NativeWorkspace) -> WorkSchedule {
        let Ok(services) = self.services() else {
            return WorkSchedule::Finished;
        };
        let attempt = {
            let mut admission = self.admission.lock().unwrap_or_else(|e| e.into_inner());
            if !admission.queue.is_empty() {
                return WorkSchedule::After(Duration::ZERO);
            }
            match admission.policy.begin_park(services.clock.now()) {
                Ok(attempt) => attempt,
                Err(_) => return WorkSchedule::Dormant,
            }
        };
        let result = (|| {
            let max = self.protected_bytes;
            let memory = IoMemory::acquire(
                &self.budgets,
                workspace.config.checkpoint_bytes,
                self.protected_bytes,
            )?;
            let disk = DiskLease::acquire(&self.budgets, max)?;
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
                self.admission
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .policy
                    .park_failed(error, services.clock.now());
                return WorkSchedule::After(self.options.retry_after);
            }
        };
        let store = services.store.clone();
        let protector = services.protector.clone();
        let key = CheckpointKey {
            lifetime: attempt.processed.lifetime,
            generation: attempt.generation,
        };
        let descriptor = checkpoint.descriptor.clone();
        let max = self.protected_bytes;
        // A commit always resolves to some CommitOutcome; the distinction between
        // Rejected (nothing was stored) and Uncertain (something may have been)
        // is the job's to make, so it is carried in the value, not in an error.
        let job = move || {
            let protected = match protector.protect(key, checkpoint) {
                Ok(protected) => protected,
                Err(error) => return Ok(CommitOutcome::Rejected(error.into())),
            };
            if protected.key != key
                || protected.descriptor != descriptor
                || protected.ciphertext().is_empty()
                || protected.ciphertext().len() > max
            {
                return Ok(CommitOutcome::Rejected(
                    ProjectionError::InvalidConfiguration,
                ));
            }
            let len = protected.ciphertext().len();
            let reference = match store.commit(&protected) {
                Ok(reference) => reference,
                Err(error) => return Ok(CommitOutcome::Rejected(error.into())),
            };
            if reference.key != key || reference.bytes != len {
                return Ok(CommitOutcome::Uncertain(
                    ProjectionError::InvalidConfiguration,
                ));
            }
            Ok(CommitOutcome::Published(reference))
        };
        match self.submit_io(job) {
            Ok(mailbox) => {
                workspace.io = Some(PendingIo::Commit {
                    attempt,
                    disk,
                    _memory: memory,
                    mailbox,
                })
            }
            Err(error) => {
                self.admission
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .policy
                    .park_failed(error, services.clock.now());
                return WorkSchedule::After(self.options.retry_after);
            }
        }
        WorkSchedule::Dormant
    }
    pub(super) fn start_delete(&self, workspace: &mut NativeWorkspace) -> WorkSchedule {
        let Ok(services) = self.services() else {
            return WorkSchedule::Finished;
        };
        let Some((source, attempts)) = workspace.pending_deletes.pop_front() else {
            return WorkSchedule::Dormant;
        };
        if attempts >= self.options.max_park_attempts {
            workspace.pending_deletes.push_front((source, attempts));
            return WorkSchedule::Dormant;
        }
        let store = services.store.clone();
        let reference = source.reference;
        match self.submit_io(move || store.delete(reference).map_err(ProjectionError::from)) {
            Ok(mailbox) => {
                workspace.io = Some(PendingIo::Delete {
                    source,
                    attempts,
                    mailbox,
                });
                WorkSchedule::Dormant
            }
            Err(_) => {
                workspace.pending_deletes.push_front((source, attempts));
                WorkSchedule::After(Duration::from_millis(5))
            }
        }
    }
}
