use super::{
    ProjectionCoordinator, ProjectionError,
    budgets::{DiskLease, IoMemory, Lease},
    state::{BlockingJob, Command, CommitOutcome, IoMailbox, IoResult, NativeWorkspace, PendingIo},
};
use crate::scheduling::WorkSchedule;
use pty_runtime_domain::checkpoint::CheckpointKey;
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex},
    time::Duration,
};
impl ProjectionCoordinator {
    fn submit_io(
        &self,
        work: impl FnOnce() -> IoResult + Send + 'static,
    ) -> Result<IoMailbox, ProjectionError> {
        let services = self.services()?;
        let mailbox = Arc::new(Mutex::new(None));
        let output = mailbox.clone();
        let wake = self
            .handle
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let job = Box::new(move || {
            let result = catch_unwind(AssertUnwindSafe(work)).unwrap_or(IoResult::Failure);
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
        let kind = if let Some(Command::Checkpoint(request, staging)) = transfer {
            BlockingJob::Transfer {
                request,
                memory,
                _staging: staging,
            }
        } else {
            let resident =
                match Lease::shared(self.budgets.resident.clone(), workspace.config.native_bytes) {
                    Ok(resident) => resident,
                    Err(_) => return WorkSchedule::After(Duration::from_millis(5)),
                };
            BlockingJob::Restore { resident, memory }
        };
        let store = services.store.clone();
        let protector = services.protector.clone();
        let reference = source.reference;
        let descriptor = source.descriptor.clone();
        let max = self.protected_bytes;
        let plaintext_max = workspace.config.checkpoint_bytes;
        let job = move || {
            IoResult::Read((|| {
                let bytes = store.read(reference, max)?;
                if bytes.ciphertext().len() != reference.bytes {
                    return Err(ProjectionError::Storage(
                        pty_runtime_domain::checkpoint::CheckpointError::Unavailable,
                    ));
                }
                let checkpoint = protector.open(reference.key, &descriptor, bytes)?;
                if checkpoint.descriptor != descriptor
                    || checkpoint.bytes.capacity() > plaintext_max
                {
                    return Err(ProjectionError::InvalidConfiguration);
                }
                Ok(checkpoint)
            })())
        };
        match self.submit_io(job) {
            Ok(mailbox) => {
                let pending = PendingIo { kind, mailbox };
                if matches!(pending.kind, BlockingJob::Restore { .. }) {
                    let result = self
                        .admission
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .policy
                        .begin_restore();
                    if let Err(error) = result {
                        self.fail(error);
                    }
                }
                workspace.io = Some(pending);
                WorkSchedule::Dormant
            }
            Err(error) => {
                if let BlockingJob::Transfer { request, .. } = kind {
                    request.fail(error);
                }
                WorkSchedule::After(Duration::from_millis(5))
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
        let job = move || {
            let protected = match protector.protect(key, checkpoint) {
                Ok(protected) => protected,
                Err(error) => return IoResult::Commit(CommitOutcome::Rejected(error.into())),
            };
            if protected.key != key
                || protected.descriptor != descriptor
                || protected.ciphertext().is_empty()
                || protected.ciphertext().len() > max
            {
                return IoResult::Commit(CommitOutcome::Rejected(
                    ProjectionError::InvalidConfiguration,
                ));
            }
            let len = protected.ciphertext().len();
            let reference = match store.commit(&protected) {
                Ok(reference) => reference,
                Err(error) => return IoResult::Commit(CommitOutcome::Rejected(error.into())),
            };
            if reference.key != key || reference.bytes != len {
                return IoResult::Commit(CommitOutcome::Uncertain(
                    ProjectionError::InvalidConfiguration,
                ));
            }
            IoResult::Commit(CommitOutcome::Published(reference))
        };
        match self.submit_io(job) {
            Ok(mailbox) => {
                workspace.io = Some(PendingIo {
                    kind: BlockingJob::Commit {
                        attempt,
                        disk,
                        _memory: memory,
                    },
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
        match self.submit_io(move || {
            IoResult::Delete(store.delete(reference).map_err(ProjectionError::from))
        }) {
            Ok(mailbox) => {
                workspace.io = Some(PendingIo {
                    kind: BlockingJob::Delete { source, attempts },
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
