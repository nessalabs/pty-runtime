use super::{ProjectionError, ProjectionOptions, ProjectionStatus, Residency};
use crate::{ReplayCursor, SessionLifetime};
use std::time::Duration;
/// Immutable parking identity; activity changes invalidate an outstanding attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParkAttempt {
    /// Unique operation generation within this session lifetime.
    pub generation: u64,
    /// Mutation/admission generation at encoding.
    pub activity: u64,
    /// Exact processed byte position.
    pub processed: ReplayCursor,
    /// Exact successful ordered-control position.
    pub control_generation: u64,
}
/// Pure state transitions. Application serializes access and owns external work.
pub struct ProjectionPolicy {
    status: ProjectionStatus,
    options: ProjectionOptions,
    activity: u64,
    operation: u64,
    last_activity: Duration,
    retry_at: Duration,
    attempts: u32,
}
impl ProjectionPolicy {
    /// Start one fresh live model at stream position zero.
    pub fn new(
        lifetime: SessionLifetime,
        options: ProjectionOptions,
        now: Duration,
    ) -> Result<Self, ProjectionError> {
        let options = options.validate()?;
        let cursor = ReplayCursor {
            lifetime,
            offset: 0,
        };
        Ok(Self {
            status: ProjectionStatus {
                published: cursor,
                processed: cursor,
                control_generation: 0,
                residency: Residency::Resident,
                history: crate::terminal::RestorationProgress::Complete,
                skipped_history_pages: 0,
                failure: None,
                parking_failure: None,
            },
            options,
            activity: 0,
            operation: 0,
            last_activity: now,
            retry_at: now,
            attempts: 0,
        })
    }
    /// Read a consistent set of terminal facts.
    pub fn status(&self) -> ProjectionStatus {
        self.status
    }
    /// Record all-or-none parser admission, including output while projection has failed.
    pub fn admit_output(&mut self, bytes: usize, now: Duration) -> Result<(), ProjectionError> {
        if matches!(
            self.status.residency,
            Residency::Closing | Residency::Closed
        ) {
            return Err(ProjectionError::Closed);
        }
        let next = self
            .status
            .published
            .offset
            .checked_add(bytes as u64)
            .ok_or(ProjectionError::Capacity)?;
        self.record_activity(now)?;
        self.status.published.offset = next;
        Ok(())
    }
    /// Mutations invalidate a parking commit even before native execution starts.
    pub fn record_activity(&mut self, now: Duration) -> Result<(), ProjectionError> {
        if matches!(
            self.status.residency,
            Residency::Closing | Residency::Closed
        ) {
            return Err(ProjectionError::Closed);
        }
        self.activity = self
            .activity
            .checked_add(1)
            .ok_or(ProjectionError::Capacity)?;
        self.last_activity = now;
        self.retry_at = now;
        self.attempts = 0;
        Ok(())
    }
    /// Advance only after native feed succeeds; never consume unadmitted bytes.
    pub fn record_processed(&mut self, bytes: usize) -> Result<(), ProjectionError> {
        let next = self
            .status
            .processed
            .offset
            .checked_add(bytes as u64)
            .ok_or(ProjectionError::Capacity)?;
        if next > self.status.published.offset {
            return Err(ProjectionError::InvalidConfiguration);
        }
        self.status.processed.offset = next;
        Ok(())
    }
    /// Apply exactly the next control generation after both OS and model work succeed.
    pub fn record_control_applied(&mut self, generation: u64) -> Result<(), ProjectionError> {
        if self.status.control_generation.checked_add(1) != Some(generation) {
            return Err(ProjectionError::InvalidConfiguration);
        }
        self.status.control_generation = generation;
        Ok(())
    }
    /// Time remaining before eligibility, or None after bounded retries are exhausted.
    pub fn park_delay(&self, now: Duration) -> Option<Duration> {
        if self.status.residency != Residency::Resident
            || self.attempts >= self.options.max_park_attempts
        {
            return None;
        }
        Some(
            self.last_activity
                .saturating_add(self.options.park_after)
                .max(self.retry_at)
                .saturating_sub(now),
        )
    }
    /// Capture the exact current version before encoding. Caller ensures no queued work.
    pub fn begin_park(&mut self, now: Duration) -> Result<ParkAttempt, ProjectionError> {
        if self.park_delay(now) != Some(Duration::ZERO)
            || self.status.published != self.status.processed
        {
            return Err(ProjectionError::InvalidConfiguration);
        }
        self.operation = self
            .operation
            .checked_add(1)
            .ok_or(ProjectionError::Capacity)?;
        self.status.residency = Residency::Parking;
        self.attempts += 1;
        Ok(ParkAttempt {
            generation: self.operation,
            activity: self.activity,
            processed: self.status.processed,
            control_generation: self.status.control_generation,
        })
    }
    /// Atomic logical release decision after successful provider publication.
    /// Caller also checks pending observation/control work under the same aggregate lock.
    pub fn commit_park(&mut self, attempt: ParkAttempt, no_pending_work: bool) -> bool {
        let valid = self.status.residency == Residency::Parking
            && self.operation == attempt.generation
            && self.activity == attempt.activity
            && self.status.processed == attempt.processed
            && self.status.control_generation == attempt.control_generation
            && no_pending_work;
        if valid {
            self.status.residency = Residency::Parked;
            self.status.parking_failure = None;
            self.attempts = 0;
        } else if self.status.residency == Residency::Parking {
            self.status.residency = Residency::Resident;
        }
        valid
    }
    /// Failed encoding/storage retains the model and schedules at most the configured retries.
    pub fn park_failed(&mut self, error: ProjectionError, now: Duration) {
        if self.status.residency == Residency::Parking {
            self.status.residency = Residency::Resident;
        }
        self.status.parking_failure = Some(error);
        self.retry_at = now.saturating_add(self.options.retry_after);
    }
    /// Begin restoration only from an immutable parked source.
    pub fn begin_restore(&mut self) -> Result<(), ProjectionError> {
        self.require_restoration_source(&[Residency::Parked])?;
        self.status.residency = Residency::Restoring;
        self.status.history = crate::terminal::RestorationProgress::Usable;
        Ok(())
    }
    /// Record a native restoration milestone from an admitted restore operation.
    /// Late completions cannot overwrite failed or closed state.
    pub fn restoration_progress(
        &mut self,
        progress: crate::terminal::RestorationProgress,
    ) -> Result<(), ProjectionError> {
        self.require_restoration_source(&[Residency::Restoring, Residency::Usable])?;
        let additional = progress
            .skipped_pages()
            .checked_sub(self.status.history.skipped_pages())
            .ok_or(ProjectionError::InvalidConfiguration)?;
        self.status.skipped_history_pages = self
            .status
            .skipped_history_pages
            .checked_add(additional)
            .ok_or(ProjectionError::Capacity)?;
        self.status.history = progress;
        self.status.residency = if progress.is_finished() {
            Residency::Resident
        } else {
            Residency::Usable
        };
        Ok(())
    }
    fn require_restoration_source(&self, allowed: &[Residency]) -> Result<(), ProjectionError> {
        if matches!(
            self.status.residency,
            Residency::Closing | Residency::Closed
        ) {
            return Err(ProjectionError::Closed);
        }
        if let Some(error) = self.status.failure {
            return Err(error);
        }
        if !allowed.contains(&self.status.residency) {
            return Err(ProjectionError::InvalidConfiguration);
        }
        Ok(())
    }
    /// Fail only projection; existing process facts and queued output belong elsewhere.
    pub fn fail(&mut self, error: ProjectionError) {
        if !matches!(
            self.status.residency,
            Residency::Closing | Residency::Closed
        ) {
            self.status.failure = Some(error);
            self.status.residency = Residency::Failed;
        }
    }
    /// Invalidate all pending state publications immediately.
    pub fn close(&mut self) {
        if self.status.residency != Residency::Closed {
            self.status.residency = Residency::Closing;
        }
    }
    /// Report exhausted background cleanup without losing the resident model.
    pub fn maintenance_failed(&mut self, error: ProjectionError) {
        self.status.parking_failure = Some(error);
    }
    /// Record bounded teardown failure; unreclaimed ciphertext remains charged by the runtime.
    pub fn cleanup_failed(&mut self, error: ProjectionError) {
        self.status.failure = Some(error);
    }
    /// Cleanup completed after all accepted operations relinquished ownership.
    pub fn mark_closed(&mut self) {
        self.status.residency = Residency::Closed;
    }
}
