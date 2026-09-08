//! Deterministic scheduling ownership and admission contracts.
use pty_runtime_application::scheduling::{
    IBlockingExecutor, ICapacitySignal, IClock, SchedulingError,
};
use pty_runtime_infrastructure::scheduling::{
    BoundedBlockingExecutor, CondvarCapacitySignal, MonotonicSystemClock,
};
use std::{
    sync::{Arc, Barrier, mpsc},
    time::{Duration, Instant},
};
#[test]
fn blocked_job_admission_is_bounded_and_shutdown_drains_accepted_jobs() {
    let pool = Arc::new(BoundedBlockingExecutor::new(2, 1).unwrap());
    let barrier = Arc::new(Barrier::new(2));
    let b = barrier.clone();
    let (tx, rx) = mpsc::channel();
    let entered = tx.clone();
    pool.submit(Box::new(move || {
        entered.send(1).unwrap();
        b.wait();
    }))
    .unwrap();
    assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), 1);
    pool.submit(Box::new(move || tx.send(2).unwrap())).unwrap();
    assert_eq!(pool.submit(Box::new(|| {})), Err(SchedulingError::Capacity));
    let p = pool.clone();
    let (done, wait) = mpsc::channel();
    let join = std::thread::spawn(move || {
        p.shutdown();
        done.send(()).unwrap();
    });
    assert!(wait.try_recv().is_err());
    barrier.wait();
    wait.recv_timeout(Duration::from_secs(2)).unwrap();
    join.join().unwrap();
    assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), 2);
    assert_eq!(pool.submit(Box::new(|| {})), Err(SchedulingError::Closed));
}
#[test]
fn job_panic_does_not_kill_worker_and_drop_joins_work() {
    let pool = BoundedBlockingExecutor::new(2, 1).unwrap();
    let (tx, rx) = mpsc::channel();
    pool.submit(Box::new(|| panic!("injected provider panic")))
        .unwrap();
    pool.submit(Box::new(move || tx.send(()).unwrap())).unwrap();
    drop(pool);
    rx.recv_timeout(Duration::from_secs(2)).unwrap();
}
#[test]
fn capacity_generation_preserves_early_notifications() {
    let signal = CondvarCapacitySignal::default();
    let generation = signal.generation();
    signal.notify();
    let started = Instant::now();
    signal.wait_after(generation, started + Duration::from_secs(5));
    assert!(started.elapsed() < Duration::from_secs(1));
    signal.wait_after(signal.generation(), Instant::now());
    let clock = MonotonicSystemClock::default();
    let first = clock.now();
    assert!(clock.now() >= first);
}
