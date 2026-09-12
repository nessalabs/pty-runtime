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
        let mut workspace = self.lock_workspace();
        self.finish_io(&mut workspace);
        // Residency as observed *before* draining in-flight OS work.
        let residency = self.status().residency;
        if matches!(residency, Residency::Closing | Residency::Closed) {
            return self.cleanup(&mut workspace, &services);
        }
        if residency == Residency::Failed {
            return self.work_while_failed(&mut workspace, &services);
        }
        // Polling can itself fail the projection without returning a schedule,
        // so failure is re-read against the state polling leaves behind. The
        // phase below still keys off the pre-poll residency: a concurrent
        // close() lands on the next run, which close() has already woken.
        if let Some(schedule) = self.poll_inflight_operations(&mut workspace) {
            return schedule;
        }
        if self.status().failure.is_some() {
            return self.work_while_failed(&mut workspace, &services);
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
        let mut workspace = self.lock_workspace();
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
        if workspace.io.busy() {
            return WorkSchedule::Dormant;
        }
        let Some(transfer) = self.queue.take_checkpoint_if_any_work() else {
            return WorkSchedule::Dormant;
        };
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
        let command = if workspace.history_step_owed {
            None
        } else {
            self.queue.take_next_if(|command| match command {
                Command::View(..) => true,
                Command::Output(..) | Command::Resize(..) => {
                    services.terminal.capabilities().mutation_during_restore
                }
                _ => false,
            })
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
        let maintenance = if !workspace.io.busy() && workspace.reaper.holds_sources() {
            Some(self.start_delete(workspace, services))
        } else {
            None
        };
        let command = self.queue.take_next();
        if let Some(command) = command {
            return self.apply_command(workspace, command);
        }
        if let Some(schedule) = maintenance {
            return schedule;
        }
        if workspace.io.busy() {
            return WorkSchedule::Dormant;
        }
        if workspace.reaper.holds_sources() {
            return self.start_delete(workspace, services);
        }
        let delay = self.queue.park_delay(services.clock.now());
        match delay {
            Some(delay) if delay.is_zero() => self.start_park(workspace),
            Some(delay) => WorkSchedule::After(delay),
            None => WorkSchedule::Dormant,
        }
    }
    /// Seal the continuation stream once every admitted mutation has been
    /// applied and nothing can still extend it.
    ///
    /// A worker step, not an API call: the reader says it has drained with
    /// `notify_output_drained`, and this is where that claim becomes an ended
    /// stream — only after the engine is idle and the queue has settled.
    fn seal_journal_if_drained(&self, workspace: &NativeWorkspace) {
        let engine_idle = workspace.resize.is_none() && workspace.reply.is_none();
        if let Some(drain) = self.queue.settled_drain(engine_idle) {
            self.journal.end(Some(drain), None);
        }
    }

    pub(super) fn fail(&self, error: ProjectionError) {
        // Preserve every parser byte under its staging lease, but failed projection
        // cannot leave already-admitted observation/control futures hanging.
        let (rejected, drain) = self.queue.fail(error);
        self.journal.end(drain, Some(error));
        for event in rejected {
            event.fail(error);
        }
        if let Ok(services) = self.services() {
            services.capacity.notify();
        }
    }
    fn work_while_failed(
        &self,
        workspace: &mut NativeWorkspace,
        services: &super::ProjectionServices,
    ) -> WorkSchedule {
        if !workspace.io.busy() && workspace.reaper.holds_sources() {
            return self.start_delete(workspace, services);
        }
        WorkSchedule::Dormant
    }

    /// Hand the reaper the ports it needs to submit its next deletion.
    ///
    /// Everything about the deletion itself — which source, what a rejected
    /// submission costs it, what the completion will be — belongs to
    /// [`super::reaper::SourceReaper`]. This only resolves the injected ports,
    /// which is the coordinator's business and nothing else's.
    fn start_delete(
        &self,
        workspace: &mut NativeWorkspace,
        services: &super::ProjectionServices,
    ) -> WorkSchedule {
        workspace.reaper.start_delete(
            &mut workspace.io,
            services.store.clone(),
            services.blocking.as_ref(),
            self.wiring.handle(),
        )
    }
    fn cleanup(
        &self,
        workspace: &mut NativeWorkspace,
        services: &super::ProjectionServices,
    ) -> WorkSchedule {
        workspace.terminal = None;
        workspace.resident = None;
        workspace.restore_memory = None;
        workspace.reply = None;
        if let Some(schedule) = self.closing_resize(workspace) {
            return schedule;
        }
        if workspace.io.busy() {
            return WorkSchedule::Dormant;
        }
        if let Some(source) = workspace.source.take() {
            workspace.reaper.retire(source);
        }
        let (failed, retry) = self.queue.take_cleanup_retry();
        if retry {
            workspace.reaper.restore_attempts();
        }
        if workspace.reaper.holds_sources() {
            if failed.is_some() {
                // Slots were reserved before commit, so this preallocated ledger
                // cannot grow beyond its independently admitted identity limit.
                self.quotas
                    .shared
                    .absorb_unreclaimed(workspace.reaper.surrender());
            } else {
                return self.start_delete(workspace, services);
            }
        }
        let (waiters, failed) = self.queue.finish_close(failed);
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
