use super::{Completion, OutputEvent, RuntimeError, SessionOptions, SessionStatus, quota::Quota};
use crate::{
    process::{IProcessEvents, IProcessSession, OutputAcceptance},
    projection::ProjectionCoordinator,
};
use pty_runtime_domain::{
    ReplayBuffer, ReplayCursor, ReplayPage, SessionLifetime,
    process::{DrainOutcome, ExitStatus, ProcessError},
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    task::Waker,
};

pub(crate) struct State {
    pub replay: ReplayBuffer,
    pub status: SessionStatus,
    pub watchers: BTreeMap<u64, Option<Waker>>,
    pub next_watcher: u64,
    metric_active: bool,
}

/// Stable application context stored by an injected repository.
/// Domain state and live process collaborators have separate ownership.
pub struct SessionContext {
    pub(crate) diagnostics: Option<Arc<crate::diagnostics::RuntimeDiagnostics>>,
    pub(crate) lifetime: SessionLifetime,
    pub(crate) options: SessionOptions,
    pub(crate) state: Mutex<State>,
    pub(crate) process: Mutex<Option<Arc<dyn IProcessSession>>>,
    pub(crate) projection: Mutex<Option<Arc<ProjectionCoordinator>>>,
    pub(crate) observers: Arc<Quota>,
    pub(crate) replay_quota: Arc<Quota>,
    pub(crate) page_bytes: usize,
    pub(crate) input_bytes: Arc<Quota>,
    pub(crate) input_slots: Arc<Quota>,
}
impl SessionContext {
    pub(crate) fn new(
        lifetime: SessionLifetime,
        options: SessionOptions,
        observers: Arc<Quota>,
        replay_quota: Arc<Quota>,
        page_bytes: usize,
        input_bytes: Arc<Quota>,
        input_slots: Arc<Quota>,
    ) -> Self {
        Self {
            diagnostics: None,
            lifetime,
            state: Mutex::new(State {
                replay: ReplayBuffer::new(lifetime, options.replay_bytes),
                status: SessionStatus::default(),
                watchers: BTreeMap::new(),
                next_watcher: 0,
                metric_active: false,
            }),
            options,
            process: Mutex::new(None),
            projection: Mutex::new(None),
            observers,
            replay_quota,
            page_bytes,
            input_bytes,
            input_slots,
        }
    }
    pub(crate) fn with_diagnostics(
        mut self,
        diagnostics: Option<Arc<crate::diagnostics::RuntimeDiagnostics>>,
    ) -> Self {
        if let Some(diagnostics) = &diagnostics {
            diagnostics.session_activity(true);
        }
        self.state
            .get_mut()
            .unwrap_or_else(|error| error.into_inner())
            .metric_active = diagnostics.is_some();
        self.diagnostics = diagnostics;
        self
    }
    pub(crate) fn timing(
        &self,
        kind: crate::diagnostics::LatencyKind,
    ) -> Option<crate::diagnostics::Timing> {
        self.diagnostics.as_ref().map(|diagnostics| {
            crate::diagnostics::Timing::new(diagnostics.clone(), kind, std::time::Instant::now())
        })
    }
    /// Runtime-issued identity used for atomic repository comparisons.
    pub fn lifetime(&self) -> SessionLifetime {
        self.lifetime
    }
    /// Separate lifecycle facts; lock failures are explicit application errors.
    pub fn status(&self) -> Result<SessionStatus, RuntimeError> {
        Ok(self
            .state
            .lock()
            .map_err(|_| RuntimeError::Internal)?
            .status)
    }
    /// Finished means process supervision and output drain are both terminal.
    pub fn completion(&self) -> Result<Option<Completion>, RuntimeError> {
        Ok(self.status()?.completion())
    }

    pub(crate) fn register_watcher(&self) -> Result<u64, RuntimeError> {
        if !self.observers.acquire(1) {
            return Err(RuntimeError::Capacity);
        }
        let result = (|| {
            let mut state = self.state.lock().map_err(|_| RuntimeError::Internal)?;
            if state.watchers.len() >= self.options.max_observers {
                return Err(RuntimeError::Capacity);
            }
            let id = state.next_watcher;
            state.next_watcher = id.checked_add(1).ok_or(RuntimeError::Capacity)?;
            state.watchers.insert(id, None);
            Ok(id)
        })();
        if result.is_err() {
            self.observers.release(1);
        }
        result
    }
    pub(crate) fn remove_watcher(&self, id: u64) {
        let removed = self
            .state
            .lock()
            .ok()
            .and_then(|mut state| state.watchers.remove(&id));
        if removed.is_some() {
            self.observers.release(1);
        }
        // Wakers may own another observer and reenter this same session on Drop.
        drop(removed);
    }
    pub(crate) fn clear_waker(&self, id: u64) {
        let old = self
            .state
            .lock()
            .ok()
            .and_then(|mut state| state.watchers.get_mut(&id).and_then(Option::take));
        drop(old);
    }
    pub(crate) fn read(
        state: &State,
        cursor: ReplayCursor,
        page_bytes: usize,
    ) -> Result<Option<OutputEvent>, RuntimeError> {
        match state
            .replay
            .read(cursor, page_bytes)
            .map_err(|_| RuntimeError::InvalidCursor)?
        {
            ReplayPage::Pending => Ok(state.status.completion().map(OutputEvent::Complete)),
            page => Ok(Some(OutputEvent::Replay(page))),
        }
    }
    pub(crate) fn update_and_notify(&self, change: impl FnOnce(&mut State)) {
        let wakers = if let Ok(mut state) = self.state.lock() {
            change(&mut state);
            if state.metric_active && state.status.completion().is_some() {
                state.metric_active = false;
                if let Some(diagnostics) = &self.diagnostics {
                    diagnostics.session_activity(false);
                }
            }
            state
                .watchers
                .values_mut()
                .filter_map(Option::take)
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        for waker in wakers {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| waker.wake()));
        }
    }
    pub(crate) fn projection(&self) -> Result<Option<Arc<ProjectionCoordinator>>, RuntimeError> {
        Ok(self
            .projection
            .lock()
            .map_err(|_| RuntimeError::Internal)?
            .clone())
    }
    pub(crate) fn process(&self) -> Result<Arc<dyn IProcessSession>, RuntimeError> {
        self.process
            .lock()
            .map_err(|_| RuntimeError::Internal)?
            .as_ref()
            .cloned()
            .ok_or(RuntimeError::Closed)
    }
}
impl Drop for SessionContext {
    fn drop(&mut self) {
        // Drop owns the context exclusively; poison does not invalidate its
        // allocations. Release reservations even after a collaborator panic.
        let state = self
            .state
            .get_mut()
            .unwrap_or_else(|error| error.into_inner());
        self.replay_quota.release(state.replay.allocated_bytes());
        self.observers.release(state.watchers.len());
        if let Some(diagnostics) = &self.diagnostics {
            if state.metric_active {
                diagnostics.session_activity(false);
            }
            diagnostics.replay_retention(state.replay.len(), 0);
        }
    }
}

pub(crate) struct SessionEventSink(pub std::sync::Weak<SessionContext>);
impl IProcessEvents for SessionEventSink {
    fn diagnostics(&self) -> Option<Arc<crate::diagnostics::RuntimeDiagnostics>> {
        self.0
            .upgrade()
            .and_then(|context| context.diagnostics.clone())
    }
    fn output(&self, bytes: &[u8]) -> OutputAcceptance {
        self.output_observed(bytes, None)
    }
    fn output_observed(
        &self,
        bytes: &[u8],
        read_completed: Option<std::time::Instant>,
    ) -> OutputAcceptance {
        let Some(context) = self.0.upgrade() else {
            return OutputAcceptance::Closed;
        };
        let Ok(mut state) = context.state.lock() else {
            return OutputAcceptance::Closed;
        };
        let projection = match context.projection() {
            Ok(projection) => projection,
            Err(_) => return OutputAcceptance::Closed,
        };
        if let Some(projection) = projection {
            let observed = context.diagnostics.clone().zip(read_completed);
            match projection.stage_output_observed(bytes, observed) {
                OutputAcceptance::Accepted => (),
                other => return other,
            }
        }
        let retained_before = state.replay.len();
        let capacity = state.replay.allocated_bytes();
        let needed = state
            .replay
            .len()
            .saturating_add(bytes.len())
            .min(context.options.replay_bytes);
        let target = needed
            .saturating_add(4095)
            .saturating_div(4096)
            .saturating_mul(4096)
            .min(context.options.replay_bytes);
        if target > capacity && context.replay_quota.acquire(target - capacity) {
            match state.replay.reserve_capacity(target) {
                Ok(actual) if actual == target => (),
                Ok(actual) => {
                    // Standard collections may report excess capacity; charge it before use.
                    if actual > target && !context.replay_quota.acquire(actual - target) {
                        state.replay.release_storage();
                        context.replay_quota.release(target);
                    }
                    if actual < target {
                        context.replay_quota.release(target - actual);
                    }
                }
                Err(_) => context.replay_quota.release(target - capacity),
            }
        }
        let allowed = state.replay.allocated_bytes();
        if state.replay.append_with_limit(bytes, allowed).is_err() {
            return OutputAcceptance::Closed;
        }
        if let Some(diagnostics) = &context.diagnostics {
            diagnostics.replay_retention(retained_before, state.replay.len());
            diagnostics.count(
                crate::diagnostics::CounterKind::PublishedBytes,
                bytes.len() as u64,
            );
        }
        let wakers = state
            .watchers
            .values_mut()
            .filter_map(Option::take)
            .collect::<Vec<_>>();
        drop(state);
        if let (Some(diagnostics), Some(read_completed)) = (&context.diagnostics, read_completed) {
            diagnostics.record(
                crate::diagnostics::LatencyKind::RawOutput,
                read_completed.elapsed(),
                true,
            );
        }
        for waker in wakers {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| waker.wake()));
        }
        OutputAcceptance::Accepted
    }
    fn wait_for_capacity(&self, deadline: std::time::Instant) {
        if let Some(context) = self.0.upgrade() {
            if let Ok(Some(projection)) = context.projection() {
                projection.wait_for_capacity(deadline);
            }
        }
    }
    fn exited(&self, status: ExitStatus) {
        if let Some(context) = self.0.upgrade() {
            context.update_and_notify(|state| {
                if state.status.record_exit(status).is_err() {
                    state.status.record_failure(ProcessError::Internal);
                }
            });
        }
    }
    fn drained(&self, outcome: DrainOutcome) {
        if let Some(context) = self.0.upgrade() {
            context.update_and_notify(|state| {
                if state.status.record_drain(outcome).is_err() {
                    state.status.record_failure(ProcessError::Internal);
                }
            });
            // Raw completion is independent of parser catchup. Seal projection only
            // after releasing the raw state lock, preserving cancellation isolation.
            if let Ok(Some(projection)) = context.projection() {
                projection.notify_output_drained(outcome);
            }
        }
    }
    fn supervision_failed(&self, error: ProcessError) {
        if let Some(context) = self.0.upgrade() {
            if let Some(diagnostics) = &context.diagnostics {
                diagnostics.count(crate::diagnostics::CounterKind::FailedOperations, 1);
            }
            context.update_and_notify(|state| {
                state.status.record_failure(error);
            });
        }
    }
}
