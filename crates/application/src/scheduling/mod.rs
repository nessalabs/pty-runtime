//! Bounded scheduling and monotonic time ports; implementations own all workers.
use std::{
    sync::{Arc, Weak},
    time::{Duration, Instant},
};

/// Scheduler admission/ownership failure without executor-specific types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulingError {
    /// All registered work or queue capacity is in use.
    Capacity,
    /// Scheduler is closed.
    Closed,
    /// Worker infrastructure failed.
    Failed,
}
/// Monotonic clock for activity and parking policy; deterministic clocks may be injected.
pub trait IClock: Send + Sync {
    /// Time elapsed within this owner clock; never wall-clock time.
    fn now(&self) -> Duration;
}
/// What a bounded unit of per-session work needs next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkSchedule {
    /// Sleep until explicitly woken by new input or operation completion.
    Dormant,
    /// Request another run after this duration; one timer per registration at most.
    After(Duration),
    /// Remove this registration after the current invocation finishes.
    Finished,
}
/// Serialized per-session work. The scheduler never runs one registration concurrently.
pub trait IScheduledWork: Send + Sync {
    /// Perform one bounded unit without sleeping on storage/OS operations.
    fn run(&self) -> WorkSchedule;
    /// Report worker panic/failure without claiming the native operation completed.
    fn failed(&self);
}
/// A coalesced wake registration; repeated wake calls never grow an unbounded queue.
pub trait IWorkHandle: Send + Sync {
    /// Schedule one run, preserving a wake arriving during a running invocation.
    /// Explicit closure or a Finished result retires the registration even if wake
    /// was accepted concurrently; acceptance is not a completion guarantee.
    fn wake(&self) -> Result<(), SchedulingError>;
    /// Reject new wakes and release the registration after in-flight work ends.
    fn close(&self);
}
/// Shared fixed worker pool plus at most one timer per registered session.
pub trait IWorkScheduler: Send + Sync {
    /// Reserve registration capacity; weak ownership prevents scheduler/session cycles.
    fn register(
        &self,
        work: Weak<dyn IScheduledWork>,
    ) -> Result<Arc<dyn IWorkHandle>, SchedulingError>;
    /// Reject registration, finish in-flight work, and join workers.
    /// From a worker callback, requests closure and returns to avoid self/cross-join;
    /// a subsequent external call joins. Callback work must eventually return.
    fn shutdown(&self);
}
/// Separate bounded pool for potentially blocking external provider operations.
pub trait IBlockingExecutor: Send + Sync {
    /// Admit once or reject; queued/running jobs remain owned when callers drop waits.
    fn submit(&self, work: Box<dyn FnOnce() + Send>) -> Result<(), SchedulingError>;
    /// Drain accepted jobs and join workers. Injected providers must have finite operations.
    /// From a pool callback, requests closure and returns; an external call joins.
    /// Dropping the final owner inside a callback lets finite accepted work finish
    /// on detached workers, so callers needing completion must shut down externally.
    fn shutdown(&self);
}
/// Generation-based capacity wakeup, preventing notification-before-wait loss.
pub trait ICapacitySignal: Send + Sync {
    /// Current notification generation.
    fn generation(&self) -> u64;
    /// Notify after releasing capacity; generation advances even with no waiter.
    fn notify(&self);
    /// Return on generation change or deadline; callers recheck their domain predicate.
    fn wait_after(&self, generation: u64, deadline: Instant);
}
