use super::{
    ProjectionCoordinator, ProjectionError, Residency,
    state::{NativeWorkspace, PendingIo, UnreclaimedSource},
};
use pty_runtime_domain::{checkpoint::CheckpointKey, terminal::CheckpointDescriptor};
use std::panic::{AssertUnwindSafe, catch_unwind};
impl ProjectionCoordinator {
    /// Durable cleanup result, independent of observer admission or abandoned waits.
    pub(crate) fn close_outcome(&self) -> Option<Result<(), ProjectionError>> {
        let admission = self.admission.lock().unwrap_or_else(|e| e.into_inner());
        (admission.policy.status().residency == Residency::Closed)
            .then(|| admission.cleanup_failure.map_or(Ok(()), Err))
    }

    /// Last-resort owner cleanup after BOTH scheduler and blocking executor shutdown
    /// have joined/drained their accepted work. Never call concurrently with a worker.
    /// A broken scheduler cannot make forgotten handles retain keys, native state or
    /// pool storage forever. Unreclaimed/uncertain sources remain quota-charged and
    /// are reported as teardown failure; no provider call occurs on this path.
    pub(crate) fn finish_after_shutdown(&self) {
        if self.status().residency == Residency::Closed {
            return;
        }
        self.journal.close();
        let rejected = {
            let mut admission = self.admission.lock().unwrap_or_else(|e| e.into_inner());
            admission.policy.close();
            admission.cleanup_failure = Some(ProjectionError::Worker);
            std::mem::take(&mut admission.queue)
        };
        for event in rejected {
            self.contain_panic(|| event.fail(ProjectionError::Worker));
        }
        let mut workspace = self.workspace.lock().unwrap_or_else(|e| e.into_inner());
        self.finish_io(&mut workspace);
        if let Some(pending) = workspace.io.take() {
            match pending {
                PendingIo::Commit { attempt, disk, .. } => {
                    let source = UnreclaimedSource {
                        _reference: None,
                        _key: CheckpointKey {
                            lifetime: attempt.processed.lifetime,
                            generation: attempt.generation,
                        },
                        _descriptor: CheckpointDescriptor {
                            compatibility: self.compatibility.clone(),
                            processed: attempt.processed,
                            control_generation: attempt.control_generation,
                        },
                        _disk: disk,
                    };
                    self.budgets
                        .unreclaimed
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .push(source);
                }
                PendingIo::Delete { source, .. } => self
                    .budgets
                    .unreclaimed
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(source.into()),
                PendingIo::Transfer { request, .. } => {
                    self.contain_panic(|| request.fail(ProjectionError::Worker))
                }
                PendingIo::Restore { .. } => (),
            }
        }
        if let Some(source) = workspace.source.take() {
            workspace.pending_deletes.push_back((source, 0));
        }
        {
            let mut ledger = self
                .budgets
                .unreclaimed
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            for (source, _) in workspace.pending_deletes.drain(..) {
                ledger.push(source.into());
            }
        }
        self.discard_operations(&mut workspace);
        let terminal = workspace.terminal.take();
        let resident = workspace.resident.take();
        let memory = workspace.restore_memory.take();
        drop(workspace);
        self.contain_panic(|| drop(terminal));
        drop(resident);
        drop(memory);
        let waiters = {
            let mut admission = self.admission.lock().unwrap_or_else(|e| e.into_inner());
            admission.policy.cleanup_failed(ProjectionError::Worker);
            admission.policy.mark_closed();
            std::mem::take(&mut admission.close_waiters)
        };
        for waiter in waiters {
            self.contain_panic(|| waiter.complete(Err(ProjectionError::Worker)));
        }
        let process = self
            .process
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        self.contain_panic(|| drop(process));
        if let Ok(services) = self.services() {
            self.contain_panic(|| services.capacity.notify());
        }
        let services = self
            .services
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        self.contain_panic(|| drop(services));
        let handle = self.handle.lock().unwrap_or_else(|e| e.into_inner()).take();
        self.contain_panic(|| drop(handle));
    }
    pub(super) fn discard_operations(&self, workspace: &mut NativeWorkspace) {
        let reply = workspace.reply.take();
        self.contain_panic(|| drop(reply));
        if let Some(resize) = workspace.resize.take() {
            self.contain_panic(|| resize.ticket.complete(Err(ProjectionError::Worker)));
            // A panicked Future must never be polled again. The backend retains any
            // admitted OS operation independently of this discarded observation wait.
            self.contain_panic(|| drop(resize));
        }
    }
    pub(super) fn contain_panic(&self, work: impl FnOnce()) {
        if catch_unwind(AssertUnwindSafe(work)).is_err() {
            self.admission
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .policy
                .fail(ProjectionError::Worker);
        }
    }
}
