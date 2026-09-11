use super::{
    ProjectionCoordinator, ProjectionError, blocking,
    budgets::{DiskLease, IoMemory, Lease},
    queue::ParkStart,
    state::{Command, Mailbox, NativeWorkspace, PendingIo},
};
use crate::scheduling::WorkSchedule;
use pty_runtime_domain::checkpoint::CheckpointKey;
use std::time::Duration;
impl ProjectionCoordinator {
    /// Submit one job to the blocking pool against this projection's executor.
    fn submit_io<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> Result<T, ProjectionError> + Send + 'static,
    ) -> Result<Mailbox<T>, ProjectionError> {
        let services = self.services()?;
        blocking::submit(services.blocking.as_ref(), self.wiring.handle(), work)
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
            &self.quotas.shared,
            workspace.config.checkpoint_bytes,
            self.config.protected_bytes,
        ) {
            Ok(memory) => memory,
            Err(error) => {
                if let Some(event) = transfer {
                    event.fail(error);
                }
                return WorkSchedule::After(Duration::from_millis(5));
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
            let resident = match Lease::shared(
                self.quotas.shared.resident.clone(),
                workspace.config.native_bytes,
            ) {
                Ok(resident) => resident,
                Err(_) => return WorkSchedule::After(Duration::from_millis(5)),
            };
            match self.submit_io(job) {
                Ok(mailbox) => {
                    let result = self.queue.begin_restore();
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
                self.queue.park_failed(error, services.clock.now());
                return WorkSchedule::After(self.config.options.retry_after);
            }
        }
        WorkSchedule::Dormant
    }
    pub(super) fn start_delete(&self, workspace: &mut NativeWorkspace) -> WorkSchedule {
        let Ok(services) = self.services() else {
            return WorkSchedule::Finished;
        };
        let Some(attempt) = workspace.reaper.check_out() else {
            return WorkSchedule::Dormant;
        };
        let job = blocking::delete_job(services.store.clone(), attempt.source.reference);
        match self.submit_io(job) {
            Ok(mailbox) => {
                workspace.io = Some(PendingIo::Delete { attempt, mailbox });
                WorkSchedule::Dormant
            }
            Err(_) => {
                workspace.reaper.return_unsubmitted(attempt);
                WorkSchedule::After(Duration::from_millis(5))
            }
        }
    }
}
