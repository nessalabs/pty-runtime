use super::{
    ProjectionCoordinator, ProjectionError,
    budgets::{DiskLease, IoMemory, Lease},
    state::{CommitOutcome, Engine, Event, IoKind, IoMailbox, IoResult, PendingIo},
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
    pub(super) fn start_read(&self, engine: &mut Engine, transfer: Option<Event>) -> WorkSchedule {
        let Ok(services) = self.services() else {
            return WorkSchedule::Finished;
        };
        let Some(source) = &engine.source else {
            self.fail(ProjectionError::Worker);
            return WorkSchedule::Dormant;
        };
        let memory = match IoMemory::acquire(
            &self.budgets,
            engine.config.checkpoint_bytes,
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
        let kind = if let Some(Event::Checkpoint(ticket, staging)) = transfer {
            IoKind::Transfer {
                ticket,
                memory,
                _staging: staging,
            }
        } else {
            let resident =
                match Lease::one(self.budgets.resident.clone(), engine.config.native_bytes) {
                    Ok(resident) => resident,
                    Err(_) => return WorkSchedule::After(Duration::from_millis(5)),
                };
            IoKind::Restore { resident, memory }
        };
        let store = services.store.clone();
        let protector = services.protector.clone();
        let reference = source.reference;
        let descriptor = source.descriptor.clone();
        let max = self.protected_bytes;
        let plaintext_max = engine.config.checkpoint_bytes;
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
                if matches!(pending.kind, IoKind::Restore { .. }) {
                    let result = self
                        .core
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .policy
                        .begin_restore();
                    if let Err(error) = result {
                        self.fail(error);
                    }
                }
                engine.io = Some(pending);
                WorkSchedule::Dormant
            }
            Err(error) => {
                if let IoKind::Transfer { ticket, .. } = kind {
                    ticket.complete(Err(error));
                }
                WorkSchedule::After(Duration::from_millis(5))
            }
        }
    }
    pub(super) fn start_park(&self, engine: &mut Engine) -> WorkSchedule {
        let Ok(services) = self.services() else {
            return WorkSchedule::Finished;
        };
        let attempt = {
            let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
            if !core.queue.is_empty() {
                return WorkSchedule::After(Duration::ZERO);
            }
            match core.policy.begin_park(services.clock.now()) {
                Ok(attempt) => attempt,
                Err(_) => return WorkSchedule::Dormant,
            }
        };
        let result = (|| {
            let max = self.protected_bytes;
            let memory = IoMemory::acquire(
                &self.budgets,
                engine.config.checkpoint_bytes,
                self.protected_bytes,
            )?;
            let disk = DiskLease::acquire(&self.budgets, max)?;
            let terminal = engine.terminal.as_mut().ok_or(ProjectionError::Closed)?;
            let descriptor = self.descriptor();
            let checkpoint = self.native_call(|| terminal.checkpoint(descriptor.clone()))?;
            if checkpoint.descriptor != descriptor {
                return Err(ProjectionError::InvalidConfiguration);
            }
            if checkpoint.bytes.capacity() > engine.config.checkpoint_bytes {
                return Err(ProjectionError::Capacity);
            }
            Ok((checkpoint, memory, disk))
        })();
        let (checkpoint, memory, disk) = match result {
            Ok(values) => values,
            Err(error) => {
                self.core
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
                engine.io = Some(PendingIo {
                    kind: IoKind::Commit {
                        attempt,
                        disk,
                        _memory: memory,
                    },
                    mailbox,
                })
            }
            Err(error) => {
                self.core
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .policy
                    .park_failed(error, services.clock.now());
                return WorkSchedule::After(self.options.retry_after);
            }
        }
        WorkSchedule::Dormant
    }
    pub(super) fn start_delete(&self, engine: &mut Engine) -> WorkSchedule {
        let Ok(services) = self.services() else {
            return WorkSchedule::Finished;
        };
        let Some((source, attempts)) = engine.garbage.pop_front() else {
            return WorkSchedule::Dormant;
        };
        if attempts >= self.options.max_park_attempts {
            engine.garbage.push_front((source, attempts));
            return WorkSchedule::Dormant;
        }
        let store = services.store.clone();
        let reference = source.reference;
        match self.submit_io(move || {
            IoResult::Delete(store.delete(reference).map_err(ProjectionError::from))
        }) {
            Ok(mailbox) => {
                engine.io = Some(PendingIo {
                    kind: IoKind::Delete { source, attempts },
                    mailbox,
                });
                WorkSchedule::Dormant
            }
            Err(_) => {
                engine.garbage.push_front((source, attempts));
                WorkSchedule::After(Duration::from_millis(5))
            }
        }
    }
}
