use super::{
    ProjectionCoordinator, ProjectionError, Residency,
    state::{Command, NativeWorkspace},
};
use crate::scheduling::{IScheduledWork, WorkSchedule};
use std::time::Duration;
impl IScheduledWork for ProjectionCoordinator {
    fn run(&self) -> WorkSchedule {
        let Ok(services) = self.services() else {
            return WorkSchedule::Finished;
        };
        let mut workspace = self.workspace.lock().unwrap_or_else(|e| e.into_inner());
        self.finish_io(&mut workspace);
        let status = self.status();
        if matches!(status.residency, Residency::Closing | Residency::Closed) {
            return self.cleanup(&mut workspace);
        }
        if status.residency == Residency::Failed {
            return self.work_while_failed(&mut workspace);
        }
        if let Some(schedule) = self.poll_inflight_operations(&mut workspace) {
            return schedule;
        }
        if self.status().failure.is_some() {
            return self.work_while_failed(&mut workspace);
        }
        self.seal_journal_if_drained(&workspace);
        if workspace.terminal.is_none() {
            if workspace.io.is_some() {
                return WorkSchedule::Dormant;
            }
            let mut admission = self.admission.lock().unwrap_or_else(|e| e.into_inner());
            if admission.queue.is_empty() {
                return WorkSchedule::Dormant;
            }
            let transfer = if matches!(admission.queue.front(), Some(Command::Checkpoint(..))) {
                admission.queue.pop_front()
            } else {
                None
            };
            drop(admission);
            return self.start_read(&mut workspace, transfer);
        }
        if status.residency == Residency::Usable {
            let event = {
                let mut admission = self.admission.lock().unwrap_or_else(|e| e.into_inner());
                let allowed = match admission.queue.front() {
                    Some(Command::View(..)) => true,
                    Some(Command::Output(..) | Command::Resize(..)) => {
                        services.terminal.capabilities().mutation_during_restore
                    }
                    _ => false,
                };
                if allowed && !workspace.history_step_owed {
                    admission.queue.pop_front()
                } else {
                    None
                }
            };
            if let Some(event) = event {
                // One admitted operation then one history unit: neither a flood
                // nor observation churn can starve source validation indefinitely.
                workspace.history_step_owed = true;
                return self.apply_command(&mut workspace, event);
            }
            let result = match &mut workspace.terminal {
                Some(terminal) => self.native_call(|| terminal.restore_history_step()),
                None => return WorkSchedule::Dormant,
            };
            workspace.history_step_owed = false;
            match result {
                Ok(progress) => self.history_progress(&mut workspace, progress),
                Err(error) => self.fail(error),
            }
            return WorkSchedule::After(Duration::ZERO);
        }
        let maintenance = if workspace.io.is_none() && !workspace.pending_deletes.is_empty() {
            Some(self.start_delete(&mut workspace))
        } else {
            None
        };
        let event = self
            .admission
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .queue
            .pop_front();
        if let Some(event) = event {
            return self.apply_command(&mut workspace, event);
        }
        if let Some(schedule) = maintenance {
            return schedule;
        }
        if workspace.io.is_some() {
            return WorkSchedule::Dormant;
        }
        if !workspace.pending_deletes.is_empty() {
            return self.start_delete(&mut workspace);
        }
        let delay = self
            .admission
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .policy
            .park_delay(services.clock.now());
        match delay {
            Some(delay) if delay.is_zero() => self.start_park(&mut workspace),
            Some(delay) => WorkSchedule::After(delay),
            None => WorkSchedule::Dormant,
        }
    }
    fn failed(&self) -> WorkSchedule {
        self.fail(ProjectionError::Worker);
        let mut workspace = self.workspace.lock().unwrap_or_else(|e| e.into_inner());
        self.discard_operations(&mut workspace);
        match self.status().residency {
            Residency::Closing => WorkSchedule::After(Duration::ZERO),
            Residency::Closed => WorkSchedule::Finished,
            _ => WorkSchedule::Dormant,
        }
    }
}
impl ProjectionCoordinator {
    pub(super) fn fail(&self, error: ProjectionError) {
        let mut admission = self.admission.lock().unwrap_or_else(|e| e.into_inner());
        admission.policy.fail(error);
        // Preserve every parser byte under its staging lease, but failed projection
        // cannot leave already-admitted observation/control futures hanging.
        let queue = std::mem::take(&mut admission.queue);
        let mut rejected = Vec::new();
        for event in queue {
            if matches!(event, Command::Output(..)) {
                admission.queue.push_back(event);
            } else {
                rejected.push(event);
            }
        }
        let drain = admission.output_drain;
        drop(admission);
        self.journal.end(drain, Some(error));
        for event in rejected {
            event.fail(error);
        }
        if let Ok(services) = self.services() {
            services.capacity.notify();
        }
    }
    fn work_while_failed(&self, workspace: &mut NativeWorkspace) -> WorkSchedule {
        if workspace.io.is_none() && !workspace.pending_deletes.is_empty() {
            return self.start_delete(workspace);
        }
        WorkSchedule::Dormant
    }
    fn cleanup(&self, workspace: &mut NativeWorkspace) -> WorkSchedule {
        workspace.terminal = None;
        workspace.resident = None;
        workspace.restore_memory = None;
        workspace.reply = None;
        if let Some(schedule) = self.closing_resize(workspace) {
            return schedule;
        }
        if workspace.io.is_some() {
            return WorkSchedule::Dormant;
        }
        if let Some(source) = workspace.source.take() {
            workspace.pending_deletes.push_back((source, 0));
        }
        let (failed, retry) = {
            let mut admission = self.admission.lock().unwrap_or_else(|e| e.into_inner());
            (
                admission.cleanup_failure,
                std::mem::take(&mut admission.retry_cleanup),
            )
        };
        if retry {
            for (_, attempts) in &mut workspace.pending_deletes {
                *attempts = 0;
            }
        }
        if !workspace.pending_deletes.is_empty() {
            if failed.is_some() {
                // Slots were reserved before commit, so this preallocated ledger
                // cannot grow beyond its independently admitted identity limit.
                let mut ledger = self
                    .budgets
                    .unreclaimed
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                for (source, _) in workspace.pending_deletes.drain(..) {
                    ledger.push(source.into());
                }
            } else {
                return self.start_delete(workspace);
            }
        }
        let (waiters, failed) = {
            let mut admission = self.admission.lock().unwrap_or_else(|e| e.into_inner());
            let failed = failed.or(admission.unreclaimed_failure);
            admission.cleanup_failure = failed;
            if let Some(error) = failed {
                admission.policy.cleanup_failed(error);
            }
            admission.policy.mark_closed();
            (std::mem::take(&mut admission.close_waiters), failed)
        };
        self.process
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        for waiter in waiters {
            waiter.complete(failed.map_or(Ok(()), Err));
        }
        if let Ok(services) = self.services() {
            services.capacity.notify();
        }
        let services = self
            .services
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        drop(services);
        let handle = self.handle.lock().unwrap_or_else(|e| e.into_inner()).take();
        drop(handle);
        WorkSchedule::Finished
    }
}
