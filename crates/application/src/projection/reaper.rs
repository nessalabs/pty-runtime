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
    ProjectionError, blocking,
    inflight::{InFlight, PendingIo},
    state::{CommittedSource, UnreclaimedSource},
};
use crate::{
    checkpoint::ICheckpointStore,
    scheduling::{IBlockingExecutor, IWorkHandle, WorkSchedule},
};
use std::{collections::VecDeque, sync::Arc, time::Duration};

/// One source checked out for a deletion attempt.
///
/// A checkout has four possible dispositions:
///
/// ```text
///   submission rejected  -> start_delete   re-queued, same attempt count
///   delete failed        -> finish_delete  re-queued, count incremented
///   delete succeeded     -> dropped by finish_delete (releases the DiskLease)
///   shutdown mid-flight  -> surrendered to the unreclaimed ledger
/// ```
///
/// Dropping is therefore legitimate — it *is* the success path — which is why
/// this is a plain value and not an RAII guard: a guard could not tell a
/// successful delete from a mistaken drop. What keeps the two re-queueing paths
/// honest is that a checkout never leaves this module: `start_delete` hands it
/// to the slot or puts it straight back, and `finish_delete` is the only way it
/// comes out again.
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
    /// Start with nothing retired. `max_attempts` bounds how many times one
    /// source's deletion is retried before its storage is treated as
    /// unreclaimable.
    pub fn new(max_attempts: u32) -> Self {
        Self {
            queue: VecDeque::new(),
            max_attempts,
        }
    }

    /// Queue a superseded source for deletion, starting its attempt count fresh.
    ///
    /// Grows on demand rather than reserving up front. The queue is bounded —
    /// every `CommittedSource` holds a `DiskLease` charging one `stored_slots`,
    /// so it can never exceed that runtime-wide limit — but that bound is shared
    /// across all sessions, and in practice one session holds nought to two
    /// entries. Reserving the global ceiling in every session would cost
    /// hundreds of kilobytes each for storage almost none of them will use, and
    /// would be charged against no quota.
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
    fn check_out(&mut self) -> Option<DeleteAttempt> {
        if self.queue.front()?.attempts >= self.max_attempts {
            return None;
        }
        self.queue.pop_front()
    }

    /// Start deleting the next due source, if there is one the pool will take.
    ///
    /// The whole submission lives here rather than on the coordinator because
    /// every part of it is this type's own business: which source is next, what
    /// a rejected submission means for its attempt count, and that the job's
    /// completion comes back as a `Delete`. Nothing about the projection's
    /// queue, quotas or native state is involved.
    ///
    /// `Dormant` means there was nothing to start — either the queue is empty or
    /// its head has spent its retries. A rejected submission is retried shortly
    /// with the attempt count untouched, because a full pool says nothing about
    /// the provider.
    pub fn start_delete(
        &mut self,
        io: &mut InFlight,
        store: Arc<dyn ICheckpointStore>,
        executor: &dyn IBlockingExecutor,
        wake: Option<Arc<dyn IWorkHandle>>,
    ) -> WorkSchedule {
        let Some(attempt) = self.check_out() else {
            return WorkSchedule::Dormant;
        };
        let job = blocking::delete_job(store, attempt.source.reference);
        match io.submit(executor, wake, job, attempt, |attempt, mailbox| {
            PendingIo::Delete { attempt, mailbox }
        }) {
            Ok(()) => WorkSchedule::Dormant,
            Err((attempt, _)) => {
                // Never submitted, so the count is unchanged: a full pool is not
                // evidence about the provider.
                self.queue.push_front(attempt);
                WorkSchedule::After(Duration::from_millis(5))
            }
        }
    }

    /// Settle a finished deletion.
    ///
    /// A success drops the attempt here, which releases the source's disk
    /// reservation. A failure is returned only once the source's retries are
    /// spent, which is the point at which it becomes a durable cleanup outcome
    /// rather than something the next run might still fix.
    pub fn finish_delete(
        &mut self,
        attempt: DeleteAttempt,
        result: Result<(), ProjectionError>,
    ) -> Option<ProjectionError> {
        result
            .err()
            .and_then(|error| self.give_up_or_retry(attempt, error))
    }

    /// Record a failed deletion.
    ///
    /// Returns the error only once the source's retries are spent, which is the
    /// point at which the failure becomes a durable cleanup outcome rather than
    /// something the next run might still fix.
    fn give_up_or_retry(
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
    /// Drains in place, so the queue keeps whatever capacity it had grown to
    /// and a later `retire` does not have to reallocate.
    pub fn surrender(&mut self) -> impl Iterator<Item = UnreclaimedSource> + '_ {
        self.queue.drain(..).map(|attempt| attempt.source.into())
    }
}
