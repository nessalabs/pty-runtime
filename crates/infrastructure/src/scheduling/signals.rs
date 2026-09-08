use pty_runtime_application::scheduling::{ICapacitySignal, IClock};
use std::{
    sync::{Condvar, Mutex},
    time::{Duration, Instant},
};

/// Elapsed monotonic time from construction, independent of wall-clock adjustments.
pub struct MonotonicSystemClock(Instant);
impl Default for MonotonicSystemClock {
    fn default() -> Self {
        Self(Instant::now())
    }
}
impl IClock for MonotonicSystemClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }
}
/// Capacity notifications retained as a generation even before a waiter arrives.
#[derive(Default)]
pub struct CondvarCapacitySignal {
    generation: Mutex<u64>,
    changed: Condvar,
}
impl ICapacitySignal for CondvarCapacitySignal {
    fn generation(&self) -> u64 {
        *self.generation.lock().unwrap_or_else(|e| e.into_inner())
    }
    fn notify(&self) {
        let mut generation = self.generation.lock().unwrap_or_else(|e| e.into_inner());
        *generation = generation.wrapping_add(1);
        self.changed.notify_all();
    }
    fn wait_after(&self, generation: u64, deadline: Instant) {
        let mut current = self.generation.lock().unwrap_or_else(|e| e.into_inner());
        while *current == generation {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                break;
            };
            current = self
                .changed
                .wait_timeout(current, remaining)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }
}
