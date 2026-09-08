//! Independent scheduler failure-containment and invalid-admission regressions.
use pty_runtime_application::scheduling::{
    IScheduledWork, IWorkScheduler, SchedulingError, WorkSchedule,
};
use pty_runtime_infrastructure::scheduling::{BoundedBlockingExecutor, StdWorkScheduler};
use std::{
    panic::catch_unwind,
    sync::{Arc, Barrier, mpsc},
    time::Duration,
};

#[test]
fn impossible_registration_bounds_return_errors_without_panicking() {
    let result = catch_unwind(|| StdWorkScheduler::with_workers(usize::MAX, 1));
    assert!(
        matches!(result, Ok(Err(SchedulingError::Capacity))),
        "invalid capacity panicked instead of returning Capacity"
    );
}
#[test]
fn impossible_blocking_queue_bound_returns_error_without_panicking() {
    let result = catch_unwind(|| BoundedBlockingExecutor::new(usize::MAX, 1));
    assert!(
        matches!(result, Ok(Err(SchedulingError::Capacity))),
        "invalid queue capacity panicked instead of returning Capacity"
    );
}

struct DropFailure {
    entered: mpsc::Sender<()>,
    release: Arc<Barrier>,
}
impl IScheduledWork for DropFailure {
    fn run(&self) -> WorkSchedule {
        self.entered.send(()).unwrap();
        self.release.wait();
        WorkSchedule::Dormant
    }
    fn failed(&self) {}
}
impl Drop for DropFailure {
    fn drop(&mut self) {
        panic!("synthetic scheduled-owner destructor failure");
    }
}
struct Good(mpsc::Sender<()>);
impl IScheduledWork for Good {
    fn run(&self) -> WorkSchedule {
        self.0.send(()).unwrap();
        WorkSchedule::Finished
    }
    fn failed(&self) {}
}

#[test]
fn callback_owner_destructor_panic_does_not_destroy_worker_capacity() {
    let pool = StdWorkScheduler::with_workers(2, 1).unwrap();
    let (entered, wait) = mpsc::channel();
    let release = Arc::new(Barrier::new(2));
    let work: Arc<dyn IScheduledWork> = Arc::new(DropFailure {
        entered,
        release: release.clone(),
    });
    let handle = pool.register(Arc::downgrade(&work)).unwrap();
    handle.wake().unwrap();
    wait.recv_timeout(Duration::from_secs(2)).unwrap();
    // Worker now holds the only strong reference; its final drop triggers the fault.
    drop(work);
    let (done, received) = mpsc::channel();
    let good: Arc<dyn IScheduledWork> = Arc::new(Good(done));
    let next = pool.register(Arc::downgrade(&good)).unwrap();
    next.wake().unwrap();
    release.wait();
    let progressed = received.recv_timeout(Duration::from_secs(2)).is_ok();
    pool.shutdown();
    assert!(
        progressed,
        "owner destructor panic killed the only scheduler worker"
    );
}

#[test]
fn excessive_worker_counts_are_rejected_before_thread_creation() {
    for workers in [65, usize::MAX] {
        assert!(matches!(
            StdWorkScheduler::with_workers(1, workers),
            Err(SchedulingError::Capacity)
        ));
        assert!(matches!(
            BoundedBlockingExecutor::new(1, workers),
            Err(SchedulingError::Capacity)
        ));
    }
}
