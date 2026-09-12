use super::{
    ProjectionCoordinator, ProjectionError, blocking,
    budgets::{DiskLease, IoMemory, Lease},
    inflight::PendingIo,
    queue::ParkStart,
    state::{Command, NativeWorkspace},
};
use crate::scheduling::WorkSchedule;
use pty_runtime_domain::checkpoint::CheckpointKey;
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
}
