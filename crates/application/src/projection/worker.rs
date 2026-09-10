use super::{
    ProjectionCoordinator, ProjectionError, Residency,
    state::{Command, NativeWorkspace},
};
use crate::scheduling::{IScheduledWork, WorkSchedule};
use std::time::Duration;
/// What this run of the worker is for, once the terminal states are ruled out.
///
/// ```text
///   Closing/Closed ─▶ cleanup             (handled before this enum)
///   Failed         ─▶ work_while_failed   (handled before this enum)
///   terminal: None ─▶ Detached   read the committed source back
///   Usable         ─▶ Restoring  interleave history steps with commands
///   otherwise      ─▶ Serving    drain queue, reap sources, consider parking
/// ```
enum Phase {
    Detached,
    Restoring,
    Serving,
}
impl Phase {
    fn of(residency: Residency, workspace: &NativeWorkspace) -> Self {
        if workspace.terminal.is_none() {
            Self::Detached
        } else if residency == Residency::Usable {
            Self::Restoring
        } else {
            Self::Serving
        }
    }
}
impl IScheduledWork for ProjectionCoordinator {
    fn run(&self) -> WorkSchedule {
        let Ok(services) = self.services() else {
            return WorkSchedule::Finished;
        };
        let mut workspace = self.workspace.lock().unwrap_or_else(|e| e.into_inner());
        self.finish_io(&mut workspace);
        // Residency as observed *before* draining in-flight OS work.
        let residency = self.status().residency;
        if matches!(residency, Residency::Closing | Residency::Closed) {
            return self.cleanup(&mut workspace);
        }
        if residency == Residency::Failed {
            return self.work_while_failed(&mut workspace);
        }
        // Polling can itself fail the projection without returning a schedule,
        // so failure is re-read against the state polling leaves behind. The
        // phase below still keys off the pre-poll residency: a concurrent
        // close() lands on the next run, which close() has already woken.
        if let Some(schedule) = self.poll_inflight_operations(&mut workspace) {
            return schedule;
        }
        if self.status().failure.is_some() {
            return self.work_while_failed(&mut workspace);
        }
        self.seal_journal_if_drained(&workspace);
        match Phase::of(residency, &workspace) {
            Phase::Detached => self.read_back_source(&mut workspace),
            Phase::Restoring => self.restore_step(&mut workspace, &services),
            Phase::Serving => self.serve(&mut workspace, &services),
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
    /// No live model: the committed source has to be read back before anything
    /// else can run. A queued checkpoint at the head is handed to the read so a
    /// transfer can be served straight from the parked source without first
    /// restoring a native owner.
    fn read_back_source(&self, workspace: &mut NativeWorkspace) -> WorkSchedule {
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
        self.start_read(workspace, transfer)
    }
    /// Active screens are observable but history is still being validated.
    ///
    /// One admitted command then one history unit, strictly alternating: neither
    /// an output flood nor observation churn can starve source validation, and
    /// validation cannot starve observers. Engines without
    /// `mutation_during_restore` admit only views until history finishes.
    fn restore_step(
        &self,
        workspace: &mut NativeWorkspace,
        services: &super::ProjectionServices,
    ) -> WorkSchedule {
        let command = {
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
        if let Some(command) = command {
            workspace.history_step_owed = true;
            return self.apply_command(workspace, command);
        }
        let result = match &mut workspace.terminal {
            Some(terminal) => self.native_call(|| terminal.restore_history_step()),
            None => return WorkSchedule::Dormant,
        };
        workspace.history_step_owed = false;
        match result {
            Ok(progress) => self.history_progress(workspace, progress),
            Err(error) => self.fail(error),
        }
        WorkSchedule::After(Duration::ZERO)
    }
    /// Normal service. Admitted commands come first so queued work is never
    /// delayed by maintenance; source deletion only starts when the queue is
    /// empty, and parking only when there is no work and nothing to reap.
    fn serve(
        &self,
        workspace: &mut NativeWorkspace,
        services: &super::ProjectionServices,
    ) -> WorkSchedule {
        let maintenance = if workspace.io.is_none() && workspace.reaper.holds_sources() {
            Some(self.start_delete(workspace))
        } else {
            None
        };
        let command = self
            .admission
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .queue
            .pop_front();
        if let Some(command) = command {
            return self.apply_command(workspace, command);
        }
        if let Some(schedule) = maintenance {
            return schedule;
        }
        if workspace.io.is_some() {
            return WorkSchedule::Dormant;
        }
        if workspace.reaper.holds_sources() {
            return self.start_delete(workspace);
        }
        let delay = self
            .admission
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .policy
            .park_delay(services.clock.now());
        match delay {
            Some(delay) if delay.is_zero() => self.start_park(workspace),
            Some(delay) => WorkSchedule::After(delay),
            None => WorkSchedule::Dormant,
        }
    }
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
        if workspace.io.is_none() && workspace.reaper.holds_sources() {
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
            workspace.reaper.retire(source);
        }
        let (failed, retry) = {
            let mut admission = self.admission.lock().unwrap_or_else(|e| e.into_inner());
            (
                admission.cleanup_failure,
                std::mem::take(&mut admission.retry_cleanup),
            )
        };
        if retry {
            workspace.reaper.restore_attempts();
        }
        if workspace.reaper.holds_sources() {
            if failed.is_some() {
                // Slots were reserved before commit, so this preallocated ledger
                // cannot grow beyond its independently admitted identity limit.
                let mut ledger = self
                    .quotas
                    .shared
                    .unreclaimed
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                ledger.extend(workspace.reaper.surrender());
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
        self.wiring.take_process();
        for waiter in waiters {
            waiter.complete(failed.map_or(Ok(()), Err));
        }
        if let Ok(services) = self.services() {
            services.capacity.notify();
        }
        let services = self.wiring.take_services();
        drop(services);
        let handle = self.wiring.take_handle();
        drop(handle);
        WorkSchedule::Finished
    }
}
