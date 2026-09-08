//! Deterministic scheduling ownership and admission contracts.
use pty_runtime_application::scheduling::{
    IScheduledWork, IWorkScheduler, SchedulingError, WorkSchedule,
};
use pty_runtime_infrastructure::scheduling::StdWorkScheduler;
use std::sync::{Arc, Barrier, Mutex, mpsc};
use std::time::Duration;
struct Work {
    run: Box<dyn Fn() -> WorkSchedule + Send + Sync>,
    failed: Box<dyn Fn() + Send + Sync>,
}
impl IScheduledWork for Work {
    fn run(&self) -> WorkSchedule {
        (self.run)()
    }
    fn failed(&self) {
        (self.failed)()
    }
}
fn work(run: impl Fn() -> WorkSchedule + Send + Sync + 'static) -> Arc<dyn IScheduledWork> {
    Arc::new(Work {
        run: Box::new(run),
        failed: Box::new(|| {}),
    })
}
#[test]
fn running_wakes_coalesce_without_overlap_and_close_releases_after_run() {
    let pool = StdWorkScheduler::with_workers(1, 2).unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let (tx, rx) = mpsc::channel();
    let b = barrier.clone();
    let count = Mutex::new(0);
    let work = work(move || {
        let mut count = count.try_lock().expect("same registration overlapped");
        *count += 1;
        tx.send(*count).unwrap();
        b.wait();
        WorkSchedule::Dormant
    });
    let handle = pool.register(Arc::downgrade(&work)).unwrap();
    handle.wake().unwrap();
    assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), 1);
    for _ in 0..100 {
        handle.wake().unwrap();
    }
    barrier.wait();
    assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), 2);
    handle.close();
    assert_eq!(handle.wake(), Err(SchedulingError::Closed));
    assert!(matches!(
        pool.register(Arc::downgrade(&work)),
        Err(SchedulingError::Capacity)
    ));
    barrier.wait();
    pool.shutdown();
    assert!(rx.try_recv().is_err());
}
#[test]
fn registration_caps_handle_drop_and_weak_drop_reclaim_slots() {
    let pool = StdWorkScheduler::new(1).unwrap();
    let first = work(|| WorkSchedule::Dormant);
    let handle = pool.register(Arc::downgrade(&first)).unwrap();
    assert!(matches!(
        pool.register(Arc::downgrade(&first)),
        Err(SchedulingError::Capacity)
    ));
    drop(handle);
    let stale = pool.register(Arc::downgrade(&first)).unwrap();
    drop(first);
    let replacement = work(|| WorkSchedule::Dormant);
    let handle = pool.register(Arc::downgrade(&replacement)).unwrap();
    stale.close();
    handle.wake().unwrap();
    pool.shutdown();
    assert_eq!(handle.wake(), Err(SchedulingError::Closed));
}
#[test]
fn timer_fires_without_external_wake_and_finished_closes_handle() {
    let pool = StdWorkScheduler::new(1).unwrap();
    let (tx, rx) = mpsc::channel();
    let count = Mutex::new(0);
    let work = work(move || {
        let mut count = count.lock().unwrap();
        *count += 1;
        tx.send(*count).unwrap();
        if *count == 1 {
            WorkSchedule::After(Duration::from_millis(10))
        } else {
            WorkSchedule::Finished
        }
    });
    let handle = pool.register(Arc::downgrade(&work)).unwrap();
    handle.wake().unwrap();
    assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), 1);
    assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), 2);
    pool.shutdown();
    assert_eq!(handle.wake(), Err(SchedulingError::Closed));
}
#[test]
fn panic_reports_failure_and_other_work_continues() {
    let pool = StdWorkScheduler::with_workers(2, 1).unwrap();
    let (tx, rx) = mpsc::channel();
    let bad: Arc<dyn IScheduledWork> = Arc::new(Work {
        run: Box::new(|| panic!("injected")),
        failed: Box::new(move || {
            tx.send(()).unwrap();
            panic!("failure reporter also panics")
        }),
    });
    let bad_handle = pool.register(Arc::downgrade(&bad)).unwrap();
    bad_handle.wake().unwrap();
    rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let (tx, rx) = mpsc::channel();
    let good = work(move || {
        tx.send(()).unwrap();
        WorkSchedule::Finished
    });
    let good_handle = pool.register(Arc::downgrade(&good)).unwrap();
    good_handle.wake().unwrap();
    rx.recv_timeout(Duration::from_secs(2)).unwrap();
    pool.shutdown();
}
#[test]
fn callback_can_request_shutdown_without_self_join() {
    let pool = Arc::new(StdWorkScheduler::with_workers(1, 2).unwrap());
    let weak = Arc::downgrade(&pool);
    let (tx, rx) = mpsc::channel();
    let work = work(move || {
        weak.upgrade().unwrap().shutdown();
        tx.send(()).unwrap();
        WorkSchedule::Finished
    });
    let handle = pool.register(Arc::downgrade(&work)).unwrap();
    handle.wake().unwrap();
    rx.recv_timeout(Duration::from_secs(2)).unwrap();
    pool.shutdown();
}
