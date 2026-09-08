use super::{
    ProjectionCoordinator, ProjectionError, Residency,
    state::{Engine, Event},
};
use crate::scheduling::{IScheduledWork, WorkSchedule};
use std::time::Duration;
impl IScheduledWork for ProjectionCoordinator {
    fn run(&self) -> WorkSchedule {
        let Ok(services) = self.services() else {
            return WorkSchedule::Finished;
        };
        let mut engine = self.engine.lock().unwrap_or_else(|e| e.into_inner());
        self.finish_io(&mut engine);
        let status = self.status();
        if matches!(status.residency, Residency::Closing | Residency::Closed) {
            return self.cleanup(&mut engine);
        }
        if status.residency == Residency::Failed {
            return self.failed_work(&mut engine);
        }
        if let Some(schedule) = self.pending_native(&mut engine) {
            return schedule;
        }
        if self.status().failure.is_some() {
            return self.failed_work(&mut engine);
        }
        self.finish_stream(&engine);
        if engine.terminal.is_none() {
            if engine.io.is_some() {
                return WorkSchedule::Dormant;
            }
            let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
            if core.queue.is_empty() {
                return WorkSchedule::Dormant;
            }
            let transfer = if matches!(core.queue.front(), Some(Event::Checkpoint(..))) {
                core.queue.pop_front()
            } else {
                None
            };
            drop(core);
            return self.start_read(&mut engine, transfer);
        }
        if status.residency == Residency::Usable {
            let event = {
                let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
                let allowed = match core.queue.front() {
                    Some(Event::View(..)) => true,
                    Some(Event::Output(..) | Event::Resize(..)) => {
                        services.terminal.capabilities().mutation_during_restore
                    }
                    _ => false,
                };
                if allowed && !engine.history_due {
                    core.queue.pop_front()
                } else {
                    None
                }
            };
            if let Some(event) = event {
                // One admitted operation then one history unit: neither a flood
                // nor observation churn can starve source validation indefinitely.
                engine.history_due = true;
                return self.native_event(&mut engine, event);
            }
            let result = match &mut engine.terminal {
                Some(terminal) => self.native_call(|| terminal.restore_history_step()),
                None => return WorkSchedule::Dormant,
            };
            engine.history_due = false;
            match result {
                Ok(progress) => self.history_progress(&mut engine, progress),
                Err(error) => self.fail(error),
            }
            return WorkSchedule::After(Duration::ZERO);
        }
        let maintenance = if engine.io.is_none() && !engine.garbage.is_empty() {
            Some(self.start_delete(&mut engine))
        } else {
            None
        };
        let event = self
            .core
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .queue
            .pop_front();
        if let Some(event) = event {
            return self.native_event(&mut engine, event);
        }
        if let Some(schedule) = maintenance {
            return schedule;
        }
        if engine.io.is_some() {
            return WorkSchedule::Dormant;
        }
        if !engine.garbage.is_empty() {
            return self.start_delete(&mut engine);
        }
        let delay = self
            .core
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .policy
            .park_delay(services.clock.now());
        match delay {
            Some(delay) if delay.is_zero() => self.start_park(&mut engine),
            Some(delay) => WorkSchedule::After(delay),
            None => WorkSchedule::Dormant,
        }
    }
    fn failed(&self) -> WorkSchedule {
        self.fail(ProjectionError::Worker);
        let mut engine = self.engine.lock().unwrap_or_else(|e| e.into_inner());
        self.discard_operations(&mut engine);
        match self.status().residency {
            Residency::Closing => WorkSchedule::After(Duration::ZERO),
            Residency::Closed => WorkSchedule::Finished,
            _ => WorkSchedule::Dormant,
        }
    }
}
impl ProjectionCoordinator {
    pub(super) fn fail(&self, error: ProjectionError) {
        let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
        core.policy.fail(error);
        // Preserve every parser byte under its staging lease, but failed projection
        // cannot leave already-admitted observation/control futures hanging.
        let queue = std::mem::take(&mut core.queue);
        let mut rejected = Vec::new();
        for event in queue {
            if matches!(event, Event::Output(..)) {
                core.queue.push_back(event);
            } else {
                rejected.push(event);
            }
        }
        let drain = core.output_drain;
        drop(core);
        self.journal.end(drain, Some(error));
        for event in rejected {
            event.fail(error);
        }
        if let Ok(services) = self.services() {
            services.capacity.notify();
        }
    }
    fn failed_work(&self, engine: &mut Engine) -> WorkSchedule {
        if engine.io.is_none() && !engine.garbage.is_empty() {
            return self.start_delete(engine);
        }
        WorkSchedule::Dormant
    }
    fn cleanup(&self, engine: &mut Engine) -> WorkSchedule {
        engine.terminal = None;
        engine.resident = None;
        engine.restore_memory = None;
        engine.reply = None;
        if let Some(schedule) = self.closing_resize(engine) {
            return schedule;
        }
        if engine.io.is_some() {
            return WorkSchedule::Dormant;
        }
        if let Some(source) = engine.source.take() {
            engine.garbage.push_back((source, 0));
        }
        let (failed, retry) = {
            let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
            (
                core.cleanup_failure,
                std::mem::take(&mut core.retry_cleanup),
            )
        };
        if retry {
            for (_, attempts) in &mut engine.garbage {
                *attempts = 0;
            }
        }
        if !engine.garbage.is_empty() {
            if failed.is_some() {
                // Slots were reserved before commit, so this preallocated ledger
                // cannot grow beyond its independently admitted identity limit.
                let mut ledger = self
                    .budgets
                    .unreclaimed
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                for (source, _) in engine.garbage.drain(..) {
                    ledger.push(source.into());
                }
            } else {
                return self.start_delete(engine);
            }
        }
        let (waiters, failed) = {
            let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
            let failed = failed.or(core.unreclaimed_failure);
            core.cleanup_failure = failed;
            if let Some(error) = failed {
                core.policy.cleanup_failed(error);
            }
            core.policy.closed();
            (std::mem::take(&mut core.close_waiters), failed)
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
