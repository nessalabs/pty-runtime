//! The admitted-work half of a projection.
//!
//! Owns what has been accepted, what the domain policy believes about it, and
//! who is waiting on cleanup. Everything here is guarded by one mutex, and the
//! fields behind it are private: callers ask for a named operation rather than
//! locking and reaching into the state themselves.
//!
//! That matters because several of these operations are only correct as a unit.
//! Committing a park, for instance, has to observe an empty queue and an idle
//! engine *and* make the release decision without releasing the lock in between;
//! a caller doing that by hand could interleave.
use super::{
    ProjectionError, Residency,
    observation::Ticket,
    state::{Admission, Command},
};
use crate::process::OutputAcceptance;
use pty_runtime_domain::{
    process::DrainOutcome,
    projection::{ParkAttempt, ProjectionStatus},
    terminal::{ControlGeneration, RestorationProgress},
};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

/// What an output chunk should become, decided under the admission lock.
pub(super) enum OutputAdmission {
    /// Queue this command.
    Queue(Command),
    /// Accept with nothing to queue, as an empty chunk does.
    Nothing,
    /// Do not accept, for this reason. No position may advance.
    Rejected(OutputAcceptance),
}

/// What admitting an output chunk actually did.
///
/// `Queued` and `Accepted` are distinct because only the first has given the
/// worker something to do. Collapsing them means either a no-op chunk wakes a
/// worker for nothing, or a real chunk is queued and never woken.
pub(super) enum OutputOutcome {
    /// Queued; the caller must wake the worker.
    Queued,
    /// Accepted without queueing; the caller must not wake.
    Accepted,
    /// Rejected with this reason.
    Rejected(OutputAcceptance),
}

/// The outcome of asking for cleanup.
pub(super) enum CloseRequest {
    /// Cleanup already finished; this is its durable result.
    AlreadyClosed(Result<(), ProjectionError>),
    /// Closure has begun; these commands were queued and must be rejected.
    ///
    /// The queued container is handed over as-is rather than collected into a
    /// fresh one: it is preallocated to the session's admission limit, and
    /// reallocating it under the lock would add an unaccounted allocation that
    /// could abort before any resource is released.
    Started(VecDeque<Command>),
}

/// Why a parking attempt could not start.
pub(super) enum ParkStart {
    /// Work is queued; parking must wait for the engine to be idle.
    Busy,
    /// The policy is not offering a park right now.
    NotDue,
    /// Encoding may begin against this exact version.
    Ready(ParkAttempt),
}

/// What a closure admitting work may ask of the state, without being handed it.
///
/// `admit` and `admit_output` run caller code under the lock so that a
/// per-command check and the push are one step. Passing `&mut Admission` in
/// would have made that a hole: any caller could then reach `policy`, the queue
/// and the drain fact directly, which is the coupling this type exists to end.
/// The closure gets these four named operations instead.
pub(super) struct Admitting<'a>(&'a mut Admission);

impl Admitting<'_> {
    /// Whether the reader has already reported its final drain, after which no
    /// further output or control may be admitted.
    pub fn is_draining(&self) -> bool {
        self.0.output_drain.is_some()
    }

    /// Current terminal-ownership state.
    pub fn residency(&self) -> Residency {
        self.0.policy.status().residency
    }

    /// Record a mutation, which invalidates any outstanding parking attempt.
    pub fn record_activity(&mut self, now: Duration) -> Result<(), ProjectionError> {
        self.0.policy.record_activity(now)
    }

    /// Record all-or-none parser admission of this many bytes.
    pub fn admit_output_bytes(
        &mut self,
        bytes: usize,
        now: Duration,
    ) -> Result<(), ProjectionError> {
        self.0.policy.admit_output(bytes, now)
    }
}

/// A held lock, paired in test builds with its lock-order registration so the
/// two are released together.
pub(super) struct Guarded<'a, T> {
    inner: MutexGuard<'a, T>,
    #[cfg(test)]
    _tier: super::tier::TierGuard,
}
impl<'a, T> Guarded<'a, T> {
    pub(super) fn new(inner: MutexGuard<'a, T>, #[cfg(test)] tier: super::tier::TierGuard) -> Self {
        Self {
            inner,
            #[cfg(test)]
            _tier: tier,
        }
    }
}
impl<T> std::ops::Deref for Guarded<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.inner
    }
}
impl<T> std::ops::DerefMut for Guarded<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

pub(super) struct AdmissionQueue {
    state: Mutex<Admission>,
}

impl AdmissionQueue {
    pub fn new(state: Admission) -> Self {
        Self {
            state: Mutex::new(state),
        }
    }

    fn lock(&self) -> Guarded<'_, Admission> {
        #[cfg(test)]
        let _tier = super::tier::enter(super::tier::Tier::Admission);
        Guarded {
            inner: self.state.lock().unwrap_or_else(|e| e.into_inner()),
            #[cfg(test)]
            _tier,
        }
    }

    // ---- reading -----------------------------------------------------------

    /// One consistent snapshot of positions, residency and failure.
    pub fn status(&self) -> ProjectionStatus {
        self.lock().policy.status()
    }

    /// Time until parking is eligible, or `None` if it is not on offer.
    pub fn park_delay(&self, now: Duration) -> Option<Duration> {
        self.lock().policy.park_delay(now)
    }

    /// Durable cleanup result, or `None` while cleanup is still outstanding.
    pub fn close_outcome(&self) -> Option<Result<(), ProjectionError>> {
        let state = self.lock();
        (state.policy.status().residency == Residency::Closed)
            .then(|| state.cleanup_failure.map_or(Ok(()), Err))
    }

    fn check_accepting(state: &Admission) -> Result<(), ProjectionError> {
        let status = state.policy.status();
        if matches!(status.residency, Residency::Closing | Residency::Closed) {
            return Err(ProjectionError::Closed);
        }
        if let Some(error) = status.failure {
            return Err(error);
        }
        Ok(())
    }

    // ---- admitting ---------------------------------------------------------

    /// Admit one command, applying any per-command checks under the same lock.
    ///
    /// The guard is dropped before returning, because the caller's next step is
    /// to wake the worker and waking re-enters this type through `fail`.
    pub fn admit(
        &self,
        build: impl FnOnce(&mut Admitting<'_>) -> Result<Command, ProjectionError>,
    ) -> Result<(), ProjectionError> {
        let mut state = self.lock();
        Self::check_accepting(&state)?;
        let command = build(&mut Admitting(&mut state))?;
        state.queue.push_back(command);
        Ok(())
    }

    /// Admit reader output, which reports acceptance rather than failing.
    ///
    /// `accept` runs under the lock and returns the rejection reason itself, so
    /// the precedence between "closed" and "no capacity" is decided against one
    /// consistent view of the state rather than across two.
    ///
    /// "Accepted" and "queued" are deliberately distinct outcomes: an empty
    /// chunk is accepted without being queued, and must not cause the caller to
    /// wake a worker that has nothing to do.
    pub fn admit_output(
        &self,
        accept: impl FnOnce(&mut Admitting<'_>) -> OutputAdmission,
    ) -> OutputOutcome {
        let mut state = self.lock();
        match accept(&mut Admitting(&mut state)) {
            OutputAdmission::Queue(command) => {
                state.queue.push_back(command);
                OutputOutcome::Queued
            }
            OutputAdmission::Nothing => OutputOutcome::Accepted,
            OutputAdmission::Rejected(rejection) => OutputOutcome::Rejected(rejection),
        }
    }

    /// Put a command back at the head, or fail it if the projection is closing.
    pub fn requeue(&self, command: Command) {
        let mut state = self.lock();
        if matches!(
            state.policy.status().residency,
            Residency::Closing | Residency::Closed
        ) {
            drop(state);
            command.fail(ProjectionError::Closed);
        } else {
            state.queue.push_front(command);
        }
    }

    // ---- draining ----------------------------------------------------------

    /// Take the next command unconditionally.
    pub fn take_next(&self) -> Option<Command> {
        self.lock().queue.pop_front()
    }

    /// Whether any work is queued and, if the head is a checkpoint, that command.
    ///
    /// Both answers come from one acquisition. Splitting them let a concurrent
    /// `fail` drain the queue in between, after which the caller would start a
    /// restore it had decided against.
    pub fn take_checkpoint_if_any_work(&self) -> Option<Option<Command>> {
        let mut state = self.lock();
        if state.queue.is_empty() {
            return None;
        }
        Some(
            matches!(state.queue.front(), Some(Command::Checkpoint(..)))
                .then(|| state.queue.pop_front())
                .flatten(),
        )
    }

    /// Take the head only if `allowed` accepts it.
    pub fn take_next_if(&self, allowed: impl FnOnce(&Command) -> bool) -> Option<Command> {
        let mut state = self.lock();
        let take = state.queue.front().is_some_and(allowed);
        take.then(|| state.queue.pop_front()).flatten()
    }

    // ---- park lifecycle ----------------------------------------------------

    /// Begin encoding only against a quiet queue and an eligible policy.
    pub fn begin_park(&self, now: Duration) -> ParkStart {
        let mut state = self.lock();
        if !state.queue.is_empty() {
            return ParkStart::Busy;
        }
        match state.policy.begin_park(now) {
            Ok(attempt) => ParkStart::Ready(attempt),
            Err(_) => ParkStart::NotDue,
        }
    }

    /// Decide release atomically with the emptiness check it depends on.
    ///
    /// `engine_idle` reports the caller's exclusively-held native state; the
    /// queue is checked here so no command can slip in between the two.
    pub fn commit_park(&self, attempt: ParkAttempt, engine_idle: bool) -> bool {
        let mut state = self.lock();
        let quiet = engine_idle && state.queue.is_empty();
        state.policy.commit_park(attempt, quiet)
    }

    /// Retain the live model and schedule a bounded retry.
    pub fn park_failed(&self, error: ProjectionError, now: Duration) {
        self.lock().policy.park_failed(error, now);
    }

    /// A commit whose outcome is unknown: storage may hold ciphertext we cannot
    /// account for, so the failure is remembered beyond the retry.
    pub fn park_outcome_uncertain(&self, error: ProjectionError, now: Duration) {
        let mut state = self.lock();
        state.unreclaimed_failure = Some(error);
        state.policy.park_failed(error, now);
    }

    /// Background cleanup exhausted its retries; the model is still live.
    pub fn maintenance_failed(&self, error: ProjectionError) {
        let mut state = self.lock();
        state.cleanup_failure = Some(error);
        state.policy.maintenance_failed(error);
    }

    // ---- restore and control ----------------------------------------------

    /// Move to restoring; only a parked source may be restored from.
    pub fn begin_restore(&self) -> Result<(), ProjectionError> {
        self.lock().policy.begin_restore()
    }

    /// Record a native restoration milestone.
    pub fn record_restoration_progress(
        &self,
        progress: RestorationProgress,
    ) -> Result<(), ProjectionError> {
        self.lock().policy.restoration_progress(progress)
    }

    /// Apply exactly the next ordered control generation.
    pub fn record_control_applied(
        &self,
        generation: ControlGeneration,
    ) -> Result<(), ProjectionError> {
        self.lock().policy.record_control_applied(generation)
    }

    /// Advance the processed position after a successful native feed.
    pub fn record_processed(&self, bytes: usize) -> Result<(), ProjectionError> {
        self.lock().policy.record_processed(bytes)
    }

    // ---- stream end --------------------------------------------------------

    /// Record the reader's final drain once. Reports an existing projection
    /// failure so the caller can seal the journal on the spot.
    pub fn record_drain(&self, outcome: DrainOutcome) -> Option<ProjectionError> {
        let mut state = self.lock();
        if state.output_drain.is_none() {
            state.output_drain = Some(outcome);
        }
        state.policy.status().failure
    }

    /// The drain outcome, if every admitted mutation has already been applied
    /// and nothing can still extend the stream. `engine_idle` reports the
    /// caller's exclusively-held native state.
    pub fn settled_drain(&self, engine_idle: bool) -> Option<DrainOutcome> {
        let state = self.lock();
        let status = state.policy.status();
        let ready = state.output_drain.is_some()
            && engine_idle
            && !matches!(status.residency, Residency::Restoring | Residency::Usable)
            && !state
                .queue
                .iter()
                .any(|command| matches!(command, Command::Output(..) | Command::Resize(..)))
            && status.processed == status.published
            && status.failure.is_none();
        ready.then_some(state.output_drain).flatten()
    }

    // ---- failure and cleanup ----------------------------------------------

    /// Fail the projection, retaining staged output but releasing every
    /// admitted observation and control wait. Returns the commands to fail and
    /// the drain fact known at that moment.
    pub fn fail(&self, error: ProjectionError) -> (Vec<Command>, Option<DrainOutcome>) {
        let mut state = self.lock();
        state.policy.fail(error);
        let queued = std::mem::take(&mut state.queue);
        let mut rejected = Vec::new();
        for command in queued {
            if matches!(command, Command::Output(..)) {
                state.queue.push_back(command);
            } else {
                rejected.push(command);
            }
        }
        (rejected, state.output_drain)
    }

    /// Request cleanup, registering `waiter` for the durable outcome.
    ///
    /// A projection that has already finished cleanup answers immediately with
    /// its recorded outcome; otherwise closure begins and the commands that were
    /// queued are handed back for rejection.
    pub fn request_close(&self, waiter: Option<&Arc<Ticket<()>>>) -> CloseRequest {
        let mut state = self.lock();
        if state.policy.status().residency == Residency::Closed {
            return CloseRequest::AlreadyClosed(state.cleanup_failure.map_or(Ok(()), Err));
        }
        state.policy.close();
        state.cleanup_failure = None;
        state.retry_cleanup = true;
        if let Some(waiter) = waiter {
            state.close_waiters.push(waiter.clone());
        }
        CloseRequest::Started(std::mem::take(&mut state.queue))
    }

    /// Mark the projection failed without draining the queue.
    ///
    /// Used when a destructor panicked: the operation is already lost, and the
    /// queue is being torn down by the caller anyway.
    pub fn mark_failed(&self, error: ProjectionError) {
        self.lock().policy.fail(error);
    }

    /// The cleanup failure carried so far, and whether an explicit close has
    /// asked for previously-abandoned sources to be retried.
    pub fn take_cleanup_retry(&self) -> (Option<ProjectionError>, bool) {
        let mut state = self.lock();
        (
            state.cleanup_failure,
            std::mem::take(&mut state.retry_cleanup),
        )
    }

    /// Close permanently, folding in any unreclaimed-storage failure. Returns
    /// the waiters to settle and the outcome to settle them with.
    pub fn finish_close(
        &self,
        failed: Option<ProjectionError>,
    ) -> (Vec<Arc<Ticket<()>>>, Option<ProjectionError>) {
        let mut state = self.lock();
        let failed = failed.or(state.unreclaimed_failure);
        state.cleanup_failure = failed;
        if let Some(error) = failed {
            state.policy.cleanup_failed(error);
        }
        state.policy.mark_closed();
        (std::mem::take(&mut state.close_waiters), failed)
    }

    /// Abandon cleanup after both pools have joined: nothing can run again, so
    /// the outcome is worker failure. Returns the queued container itself, for
    /// the same reason as [`CloseRequest::Started`].
    pub fn abandon_close(&self) -> VecDeque<Command> {
        let mut state = self.lock();
        state.policy.close();
        state.cleanup_failure = Some(ProjectionError::Worker);
        std::mem::take(&mut state.queue)
    }

    /// Settle as failed after abandoning cleanup. Returns the waiters.
    pub fn abandon_waiters(&self) -> Vec<Arc<Ticket<()>>> {
        let mut state = self.lock();
        state.policy.cleanup_failed(ProjectionError::Worker);
        state.policy.mark_closed();
        std::mem::take(&mut state.close_waiters)
    }

    /// Number of commands queued. Test-only: production code asks `is_idle`.
    #[cfg(test)]
    pub fn queued(&self) -> usize {
        self.lock().queue.len()
    }
}
