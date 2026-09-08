use super::workers::Workers;
use pty_runtime_application::scheduling::{IBlockingExecutor, SchedulingError};
use std::{
    collections::VecDeque,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Condvar, Mutex},
    thread,
};
type Job = Box<dyn FnOnce() + Send>;
struct State {
    jobs: VecDeque<Job>,
    active: usize,
    closed: bool,
}
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
    max_jobs: usize,
}
/// Separate fixed worker pool for finite blocking provider calls. Admission counts
/// both queued and executing jobs. Shutdown drains all accepted work; panicking
/// jobs are contained so other accepted jobs still run.
pub struct BoundedBlockingExecutor {
    shared: Arc<Shared>,
    workers: Workers,
}
impl BoundedBlockingExecutor {
    /// Construct positive job capacity and one to 64 workers. Unallocatable capacity
    /// returns Capacity. Thread creation failure stops and joins
    /// workers already created. Providers must guarantee operations eventually finish.
    pub fn new(max_jobs: usize, workers: usize) -> Result<Self, SchedulingError> {
        if max_jobs == 0 || workers == 0 || workers > 64 {
            return Err(SchedulingError::Capacity);
        }
        let mut jobs = VecDeque::new();
        jobs.try_reserve_exact(max_jobs)
            .map_err(|_| SchedulingError::Capacity)?;
        let owner = Self {
            shared: Arc::new(Shared {
                state: Mutex::new(State {
                    jobs,
                    active: 0,
                    closed: false,
                }),
                changed: Condvar::new(),
                max_jobs,
            }),
            workers: Workers::default(),
        };
        for _ in 0..workers {
            let shared = owner.shared.clone();
            match thread::Builder::new()
                .name("pty-blocking".into())
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
impl IBlockingExecutor for BoundedBlockingExecutor {
    fn submit(&self, work: Job) -> Result<(), SchedulingError> {
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.closed {
            return Err(SchedulingError::Closed);
        }
        if state.jobs.len() + state.active >= self.shared.max_jobs {
            return Err(SchedulingError::Capacity);
        }
        state.jobs.push_back(work);
        self.shared.changed.notify_one();
        Ok(())
    }
    fn shutdown(&self) {
        {
            let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
            state.closed = true;
            self.shared.changed.notify_all();
        }
        self.workers.join();
    }
}
impl Drop for BoundedBlockingExecutor {
    fn drop(&mut self) {
        self.shutdown();
    }
}
fn run(shared: Arc<Shared>) {
    loop {
        let work = {
            let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                if let Some(job) = state.jobs.pop_front() {
                    state.active += 1;
                    break job;
                }
                if state.closed {
                    return;
                }
                state = shared
                    .changed
                    .wait(state)
                    .unwrap_or_else(|e| e.into_inner());
            }
        };
        let _ = catch_unwind(AssertUnwindSafe(work));
        let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
        state.active -= 1;
        shared.changed.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shutdown_joins_all_workers_and_releases_worker_state() {
        let pool = BoundedBlockingExecutor::new(2, 4).unwrap();
        pool.shutdown();
        assert_eq!(Arc::strong_count(&pool.shared), 1);
        let weak = Arc::downgrade(&pool.shared);
        drop(pool);
        assert!(weak.upgrade().is_none());
    }
}
