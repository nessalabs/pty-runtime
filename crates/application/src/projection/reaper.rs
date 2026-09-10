//! Bounded deletion of committed checkpoint sources.
//!
//! A source that has been superseded — a park that was rolled back, a restore
//! that finished with it, a projection being torn down — must have its
//! ciphertext deleted before the disk reservation it holds can be released.
//! Deletion goes through an injected provider and can fail, so it is retried a
//! bounded number of times and then given up on.
//!
//! Giving up does not release the reservation. A source we cannot prove we
//! deleted stays charged against the runtime's quota for the rest of its life,
//! which is what the unreclaimed ledger is for.
use super::{
    ProjectionError,
    state::{CommittedSource, UnreclaimedSource},
};
use std::collections::VecDeque;

/// One source checked out for a deletion attempt.
///
/// The reaper hands ownership out and cannot see what happens next, so every
/// checkout must come back through exactly one of [`SourceReaper::give_up_or_retry`]
/// (the attempt failed) or [`SourceReaper::return_unsubmitted`] (the attempt was
/// never made). Dropping one silently would leak both the ciphertext and the
/// disk reservation charged against it.
pub(super) struct DeleteAttempt {
    pub source: CommittedSource,
    pub attempts: u32,
}

/// Committed sources awaiting deletion, oldest first, each with its attempt count.
pub(super) struct SourceReaper {
    queue: VecDeque<DeleteAttempt>,
    max_attempts: u32,
}

impl SourceReaper {
    pub fn new(max_attempts: u32) -> Self {
        Self {
            queue: VecDeque::new(),
            max_attempts,
        }
    }

    /// Queue a superseded source for deletion, starting its attempt count fresh.
    pub fn retire(&mut self, source: CommittedSource) {
        self.queue.push_back(DeleteAttempt {
            source,
            attempts: 0,
        });
    }

    /// Whether any source is still held, including ones whose retries are spent.
    ///
    /// A source we have given up on stays queued and keeps this true, which is
    /// what stops a projection with unreclaimable storage from going on to park
    /// and create more of it.
    pub fn holds_sources(&self) -> bool {
        !self.queue.is_empty()
    }

    /// Take the oldest source that still has attempts left.
    ///
    /// A head whose retries are spent is left in place and `None` is returned:
    /// it is not skipped over, because deleting a newer source while an older
    /// one is stuck would reorder the provider's view of this session's storage.
    pub fn check_out(&mut self) -> Option<DeleteAttempt> {
        if self.queue.front()?.attempts >= self.max_attempts {
            return None;
        }
        self.queue.pop_front()
    }

    /// Return a checkout whose attempt was never submitted, leaving its count
    /// unchanged. A rejected submission is not evidence about the provider.
    pub fn return_unsubmitted(&mut self, attempt: DeleteAttempt) {
        self.queue.push_front(attempt);
    }

    /// Record a failed deletion.
    ///
    /// Returns the error only once the source's retries are spent, which is the
    /// point at which the failure becomes a durable cleanup outcome rather than
    /// something the next run might still fix.
    pub fn give_up_or_retry(
        &mut self,
        attempt: DeleteAttempt,
        error: ProjectionError,
    ) -> Option<ProjectionError> {
        let attempts = attempt.attempts.saturating_add(1);
        let exhausted = attempts >= self.max_attempts;
        self.queue.push_front(DeleteAttempt {
            source: attempt.source,
            attempts,
        });
        exhausted.then_some(error)
    }

    /// Allow every held source one more full round of attempts.
    ///
    /// An explicit close asks for cleanup again, so sources previously given up
    /// on are worth retrying once more before they are surrendered for good.
    pub fn restore_attempts(&mut self) {
        for attempt in &mut self.queue {
            attempt.attempts = 0;
        }
    }

    /// Hand over every held source as unreclaimed, releasing none of their
    /// reservations. Used when no further deletion will be attempted.
    pub fn surrender(&mut self) -> impl Iterator<Item = UnreclaimedSource> + use<> {
        std::mem::take(&mut self.queue)
            .into_iter()
            .map(|attempt| attempt.source.into())
    }
}
