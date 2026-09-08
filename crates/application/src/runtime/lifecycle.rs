use super::RuntimeError;
use pty_runtime_domain::SessionLifetime;
use std::sync::{Condvar, Mutex};

struct State {
    closing: bool,
    sequence: u64,
    spawns: usize,
}
pub(super) struct AdmissionGate {
    state: Mutex<State>,
    idle: Condvar,
}
impl AdmissionGate {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State {
                closing: false,
                sequence: 0,
                spawns: 0,
            }),
            idle: Condvar::new(),
        }
    }
    pub fn admit(&self, owner: u64) -> Result<(SessionLifetime, Admission<'_>), RuntimeError> {
        let mut state = self.state.lock().map_err(|_| RuntimeError::Internal)?;
        if state.closing {
            return Err(RuntimeError::Closed);
        }
        let sequence = state
            .sequence
            .checked_add(1)
            .ok_or(RuntimeError::Capacity)?;
        let spawns = state.spawns.checked_add(1).ok_or(RuntimeError::Capacity)?;
        state.sequence = sequence;
        state.spawns = spawns;
        Ok((SessionLifetime::new(owner, sequence), Admission(self)))
    }
    pub fn closing(&self) -> bool {
        self.state.lock().map_or(true, |state| state.closing)
    }
    pub fn begin_shutdown(&self) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).closing = true;
    }
    pub fn wait_for_spawns(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        while state.spawns != 0 {
            state = self.idle.wait(state).unwrap_or_else(|e| e.into_inner());
        }
    }
}
pub(super) struct Admission<'a>(&'a AdmissionGate);
impl Drop for Admission<'_> {
    fn drop(&mut self) {
        let mut state = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
        state.spawns -= 1;
        self.0.idle.notify_all();
    }
}
