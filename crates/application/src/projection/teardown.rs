use super::{
    ProjectionCoordinator, ProjectionError, Residency,
    state::{Engine, IoKind, Unreclaimed},
};
use pty_runtime_domain::{checkpoint::CheckpointKey, terminal::CheckpointDescriptor};
use std::panic::{AssertUnwindSafe, catch_unwind};
impl ProjectionCoordinator {
    /// Durable cleanup result, independent of observer admission or abandoned waits.
    pub(crate) fn close_outcome(&self) -> Option<Result<(), ProjectionError>> {
        let core = self.core.lock().unwrap_or_else(|e| e.into_inner());
        (core.policy.status().residency == Residency::Closed)
            .then(|| core.cleanup_failure.map_or(Ok(()), Err))
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
            let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
            core.policy.close();
            core.cleanup_failure = Some(ProjectionError::Worker);
            std::mem::take(&mut core.queue)
        };
        for event in rejected {
            self.drop_contained(|| event.fail(ProjectionError::Worker));
        }
        let mut engine = self.engine.lock().unwrap_or_else(|e| e.into_inner());
        self.finish_io(&mut engine);
        if let Some(pending) = engine.io.take() {
            match pending.kind {
                IoKind::Commit { attempt, disk, .. } => {
                    let compatibility = self
                        .services()
                        .map(|s| s.terminal.compatibility().to_owned())
                        .unwrap_or_default();
                    let source = Unreclaimed {
                        _reference: None,
                        _key: CheckpointKey {
                            lifetime: attempt.processed.lifetime,
                            generation: attempt.generation,
                        },
                        _descriptor: CheckpointDescriptor {
                            compatibility,
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
                IoKind::Delete { source, .. } => self
                    .budgets
                    .unreclaimed
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(source.into()),
                IoKind::Transfer { request, .. } => {
                    self.drop_contained(|| request.fail(ProjectionError::Worker))
                }
                IoKind::Restore { .. } => (),
            }
        }
        if let Some(source) = engine.source.take() {
            engine.garbage.push_back((source, 0));
        }
        {
            let mut ledger = self
                .budgets
                .unreclaimed
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            for (source, _) in engine.garbage.drain(..) {
                ledger.push(source.into());
            }
        }
        self.discard_operations(&mut engine);
        let terminal = engine.terminal.take();
        let resident = engine.resident.take();
        let memory = engine.restore_memory.take();
        drop(engine);
        self.drop_contained(|| drop(terminal));
        drop(resident);
        drop(memory);
        let waiters = {
            let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
            core.policy.cleanup_failed(ProjectionError::Worker);
            core.policy.closed();
            std::mem::take(&mut core.close_waiters)
        };
        for waiter in waiters {
            self.drop_contained(|| waiter.complete(Err(ProjectionError::Worker)));
        }
        let process = self
            .process
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        self.drop_contained(|| drop(process));
        if let Ok(services) = self.services() {
            self.drop_contained(|| services.capacity.notify());
        }
        let services = self
            .services
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        self.drop_contained(|| drop(services));
        let handle = self.handle.lock().unwrap_or_else(|e| e.into_inner()).take();
        self.drop_contained(|| drop(handle));
    }
    pub(super) fn discard_operations(&self, engine: &mut Engine) {
        let reply = engine.reply.take();
        self.drop_contained(|| drop(reply));
        if let Some(resize) = engine.resize.take() {
            self.drop_contained(|| resize.ticket.complete(Err(ProjectionError::Worker)));
            // A panicked Future must never be polled again. The backend retains any
            // admitted OS operation independently of this discarded observation wait.
            self.drop_contained(|| drop(resize));
        }
    }
    pub(super) fn drop_contained(&self, work: impl FnOnce()) {
        if catch_unwind(AssertUnwindSafe(work)).is_err() {
            self.core
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .policy
                .fail(ProjectionError::Worker);
        }
    }
}
