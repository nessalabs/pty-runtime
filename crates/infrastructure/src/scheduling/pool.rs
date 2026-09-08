use super::workers::Workers;
use pty_runtime_application::scheduling::{
    IScheduledWork, IWorkHandle, IWorkScheduler, SchedulingError, WorkSchedule,
};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Condvar, Mutex, Weak},
    thread,
    time::Instant,
};

struct Entry {
    id: u64,
    work: Weak<dyn IScheduledWork>,
    running: bool,
    pending: bool,
    closed: bool,
    due: Option<Instant>,
}
struct State {
    slots: Vec<Option<Entry>>,
    next: u64,
    cursor: usize,
    closed: bool,
}
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
}
/// Fixed workers with bounded coalesced registrations and one timer per registration.
/// Callback work must be finite and bounded. Dropping the scheduler shuts it down.
pub struct StdWorkScheduler {
    shared: Arc<Shared>,
    workers: Workers,
}
impl StdWorkScheduler {
    /// Construct with two workers and a strictly positive session capacity.
    pub fn new(max_sessions: usize) -> Result<Self, SchedulingError> {
        Self::with_workers(max_sessions, 2)
    }
    /// Configure one to 64 workers. Zero/unallocatable capacity fails; thread creation failure joins
    /// already-created workers before returning. Registration does not imply a wake.
    pub fn with_workers(max_sessions: usize, workers: usize) -> Result<Self, SchedulingError> {
        if max_sessions == 0 || workers == 0 || workers > 64 {
            return Err(SchedulingError::Capacity);
        }
        let mut slots = Vec::new();
        slots
            .try_reserve_exact(max_sessions)
            .map_err(|_| SchedulingError::Capacity)?;
        slots.resize_with(max_sessions, || None);
        let owner = Self {
            shared: Arc::new(Shared {
                state: Mutex::new(State {
                    slots,
                    next: 0,
                    cursor: 0,
                    closed: false,
                }),
                changed: Condvar::new(),
            }),
            workers: Workers::default(),
        };
        for _ in 0..workers {
            let shared = owner.shared.clone();
            match thread::Builder::new()
                .name("pty-projection".into())
                .spawn(move || run(shared))
            {
                Ok(handle) => owner.workers.add(handle),
                Err(_) => {
                    owner.shutdown();
                    return Err(SchedulingError::Failed);
                }
            }
        }
        Ok(owner)
    }
}
impl IWorkScheduler for StdWorkScheduler {
    fn register(
        &self,
        work: Weak<dyn IScheduledWork>,
    ) -> Result<Arc<dyn IWorkHandle>, SchedulingError> {
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.closed {
            return Err(SchedulingError::Closed);
        }
        for slot in &mut state.slots {
            if slot
                .as_ref()
                .is_some_and(|e| !e.running && e.work.strong_count() == 0)
            {
                *slot = None;
            }
        }
        let index = state
            .slots
            .iter()
            .position(Option::is_none)
            .ok_or(SchedulingError::Capacity)?;
        let id = state.next.checked_add(1).ok_or(SchedulingError::Failed)?;
        state.next = id;
        state.slots[index] = Some(Entry {
            id,
            work,
            running: false,
            pending: false,
            closed: false,
            due: None,
        });
        Ok(Arc::new(Handle {
            shared: self.shared.clone(),
            index,
            id,
        }))
    }
    fn shutdown(&self) {
        {
            let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
            state.closed = true;
            for slot in &mut state.slots {
                if slot.as_ref().is_some_and(|e| !e.running) {
                    *slot = None;
                } else if let Some(entry) = slot {
                    entry.closed = true;
                }
            }
            self.shared.changed.notify_all();
        }
        self.workers.join();
    }
}
impl Drop for StdWorkScheduler {
    fn drop(&mut self) {
        self.shutdown();
    }
}
struct Handle {
    shared: Arc<Shared>,
    index: usize,
    id: u64,
}
impl IWorkHandle for Handle {
    fn wake(&self) -> Result<(), SchedulingError> {
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.closed {
            return Err(SchedulingError::Closed);
        }
        let entry = state.slots[self.index]
            .as_mut()
            .filter(|e| e.id == self.id && !e.closed)
            .ok_or(SchedulingError::Closed)?;
        entry.pending = true;
        self.shared.changed.notify_one();
        Ok(())
    }
    fn close(&self) {
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = state.slots[self.index].as_mut().filter(|e| e.id == self.id) {
            if entry.running {
                entry.closed = true;
            } else {
                state.slots[self.index] = None;
            }
            self.shared.changed.notify_all();
        }
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        self.close();
    }
}
fn run(shared: Arc<Shared>) {
    loop {
        let Some((index, work)) = take(&shared) else {
            return;
        };
        let schedule = catch_unwind(AssertUnwindSafe(|| work.run()));
        let schedule = match schedule {
            Ok(schedule) => schedule,
            Err(_) => {
                catch_unwind(AssertUnwindSafe(|| work.failed())).unwrap_or(WorkSchedule::Finished)
            }
        };
        // Release arbitrary callback destructors outside the scheduler mutex.
        let schedule = if catch_unwind(AssertUnwindSafe(|| drop(work))).is_err() {
            WorkSchedule::Finished
        } else {
            schedule
        };
        let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = state.slots[index].as_mut() {
            if entry.closed || schedule == WorkSchedule::Finished {
                state.slots[index] = None;
            } else {
                entry.running = false;
                entry.due = match schedule {
                    WorkSchedule::After(delay) => Instant::now().checked_add(delay),
                    _ => None,
                };
            }
        }
        shared.changed.notify_all();
    }
}
fn take(shared: &Shared) -> Option<(usize, Arc<dyn IScheduledWork>)> {
    let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        if state.closed {
            return None;
        }
        let now = Instant::now();
        let mut nearest: Option<Instant> = None;
        let count = state.slots.len();
        let cursor = state.cursor;
        for offset in 0..count {
            let index = (cursor + offset) % count;
            let slot = &mut state.slots[index];
            let Some(entry) = slot else { continue };
            if entry.running || entry.closed {
                continue;
            }
            if entry.work.strong_count() == 0 {
                *slot = None;
                continue;
            }
            if entry.pending || entry.due.is_some_and(|due| due <= now) {
                let Some(work) = entry.work.upgrade() else {
                    *slot = None;
                    continue;
                };
                entry.pending = false;
                entry.due = None;
                entry.running = true;
                state.cursor = (index + 1) % count;
                return Some((index, work));
            }
            if let Some(due) = entry.due {
                nearest = Some(nearest.map_or(due, |previous| previous.min(due)));
            }
        }
        state = if let Some(due) = nearest {
            shared
                .changed
                .wait_timeout(state, due.saturating_duration_since(Instant::now()))
                .unwrap_or_else(|e| e.into_inner())
                .0
        } else {
            shared
                .changed
                .wait(state)
                .unwrap_or_else(|e| e.into_inner())
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shutdown_joins_all_workers_and_releases_worker_state() {
        let pool = StdWorkScheduler::with_workers(2, 4).unwrap();
        pool.shutdown();
        assert_eq!(Arc::strong_count(&pool.shared), 1);
        let weak = Arc::downgrade(&pool.shared);
        drop(pool);
        assert!(weak.upgrade().is_none());
    }
}
