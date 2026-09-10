pub use super::process::Process;
pub use super::providers::{Protector, Store};
use super::terminal::{Factory, Probe};
use crate::projection::*;
use crate::{runtime::quota::Quota, scheduling::*};
use pty_runtime_domain::{SessionLifetime, terminal::*};
use std::{
    collections::VecDeque,
    sync::{
        Arc, Condvar, Mutex, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};
#[derive(Default)]
pub struct Clock(pub AtomicU64);
impl IClock for Clock {
    fn now(&self) -> Duration {
        Duration::from_secs(self.0.load(Ordering::Acquire))
    }
}
#[derive(Default)]
pub struct Signal {
    value: Mutex<u64>,
    changed: Condvar,
}
impl ICapacitySignal for Signal {
    fn generation(&self) -> u64 {
        *self.value.lock().unwrap()
    }
    fn notify(&self) {
        *self.value.lock().unwrap() += 1;
        self.changed.notify_all();
    }
    fn wait_after(&self, old: u64, deadline: Instant) {
        let mut value = self.value.lock().unwrap();
        while *value == old {
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                return;
            };
            value = self.changed.wait_timeout(value, left).unwrap().0;
        }
    }
}
#[derive(Default)]
pub struct Scheduler;
struct Handle;
impl IWorkHandle for Handle {
    fn wake(&self) -> Result<(), SchedulingError> {
        Ok(())
    }
    fn close(&self) {}
}
impl IWorkScheduler for Scheduler {
    fn register(
        &self,
        _: Weak<dyn IScheduledWork>,
    ) -> Result<Arc<dyn IWorkHandle>, SchedulingError> {
        Ok(Arc::new(Handle))
    }
    fn shutdown(&self) {}
}
#[derive(Default)]
pub struct Jobs {
    jobs: Mutex<VecDeque<Box<dyn FnOnce() + Send>>>,
    pub reject: AtomicBool,
}
impl IBlockingExecutor for Jobs {
    fn submit(&self, work: Box<dyn FnOnce() + Send>) -> Result<(), SchedulingError> {
        if self.reject.load(Ordering::Acquire) {
            return Err(SchedulingError::Capacity);
        }
        self.jobs.lock().unwrap().push_back(work);
        Ok(())
    }
    fn shutdown(&self) {
        while self.one() {}
    }
}
impl Jobs {
    pub fn take(&self) -> Option<Box<dyn FnOnce() + Send>> {
        self.jobs.lock().unwrap().pop_front()
    }
    pub fn one(&self) -> bool {
        if let Some(work) = self.take() {
            work();
            true
        } else {
            false
        }
    }
    pub fn len(&self) -> usize {
        self.jobs.lock().unwrap().len()
    }
}
pub fn poll<T>(operation: &mut ProjectionOperation<T>) -> Poll<Result<T, ProjectionError>> {
    let waker = Waker::noop();
    operation.as_mut().poll(&mut Context::from_waker(waker))
}
pub fn result<T>(operation: &mut ProjectionOperation<T>) -> Result<T, ProjectionError> {
    match poll(operation) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("operation unexpectedly pending"),
    }
}
pub fn assert_budgets_released(h: &Harness) {
    let resources = h.budgets.resources();
    for usage in [
        resources.journal_bytes,
        resources.journal_slots,
        resources.transfer_observers,
        resources.staging_bytes,
        resources.staging_slots,
        resources.native_reservations,
        resources.checkpoint_buffers,
        resources.stored_bytes,
        resources.stored_slots,
        resources.views,
        resources.requests,
    ] {
        assert_eq!(usage.used, 0);
    }
}
pub fn options() -> ProjectionOptions {
    ProjectionOptions::new(TerminalConfig {
        size: TerminalSize::new(2, 1).unwrap(),
        history_bytes: 128,
        continuation_bytes: 128,
        reply_bytes: 32,
        checkpoint_bytes: 1024,
        native_bytes: 4096,
        view_bytes: 1024,
        feed_bytes: 16,
    })
}
pub struct Harness {
    pub owner: Arc<ProjectionCoordinator>,
    pub services: ProjectionServices,
    pub budgets: Arc<ProjectionBudgets>,
    pub clock: Arc<Clock>,
    pub jobs: Arc<Jobs>,
    pub store: Arc<Store>,
    pub protector: Arc<Protector>,
    pub probe: Arc<Probe>,
    pub process: Arc<Process>,
}
impl Harness {
    pub fn new(options: ProjectionOptions, limits: ProjectionLimits) -> Self {
        Self::with_protector(options, limits, Arc::new(Protector::default()))
    }
    pub fn with_protector(
        options: ProjectionOptions,
        limits: ProjectionLimits,
        protector: Arc<Protector>,
    ) -> Self {
        let clock = Arc::new(Clock::default());
        let jobs = Arc::new(Jobs::default());
        let store = Arc::new(Store::default());
        let probe = Arc::new(Probe::default());
        let process = Arc::new(Process::default());
        let services = ProjectionServices {
            terminal: Arc::new(Factory(probe.clone())),
            clock: clock.clone(),
            scheduler: Arc::new(Scheduler),
            blocking: jobs.clone(),
            capacity: Arc::new(Signal::default()),
            store: store.clone(),
            protector: protector.clone(),
        };
        let budgets = ProjectionBudgets::new(limits).unwrap();
        let owner = ProjectionCoordinator::create(
            SessionLifetime::new(9, 1),
            options,
            services.clone(),
            budgets.clone(),
            Arc::new(Quota::new(128)),
            Arc::new(Quota::new(8)),
        )
        .unwrap();
        owner.bind_process(process.clone()).unwrap();
        Self {
            owner,
            services,
            budgets,
            clock,
            jobs,
            store,
            protector,
            probe,
            process,
        }
    }
    pub fn standard() -> Self {
        Self::new(options(), ProjectionLimits::default())
    }
    pub fn step(&self) -> WorkSchedule {
        self.owner.run()
    }
    pub fn pump(&self) {
        for _ in 0..128 {
            let schedule = self.step();
            let io = self.jobs.one();
            if !io && !matches!(schedule,WorkSchedule::After(delay) if delay.is_zero()) {
                return;
            }
        }
        panic!("work did not reach a bounded idle state")
    }
    pub fn park(&self) {
        self.clock.0.store(60, Ordering::Release);
        self.pump();
        assert_eq!(self.owner.status().residency, Residency::Parked);
    }
    pub fn close(&self) {
        let mut wait = self.owner.close().unwrap();
        self.pump();
        result(&mut wait).unwrap();
        assert_eq!(self.owner.status().residency, Residency::Closed);
    }
}
