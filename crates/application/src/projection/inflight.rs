//! The one blocking job a projection may have outstanding, and its slot.
//!
//! A projection runs at most one blocking job at a time. That rule used to live
//! as a convention spread across the callers: each one checked the slot was
//! empty, submitted, and then remembered to store the mailbox it got back. A
//! caller that submitted and forgot to record the result would have lost the
//! job silently, and nothing but review stopped it.
//!
//! [`InFlight`] owns the slot and the submission together, so submitting *is*
//! recording: the caller hands over the work and a constructor for the variant
//! that will own its completion, and there is no way to get a mailbox without
//! filling the slot.
use super::{
    ProjectionError, blocking,
    budgets::{DiskLease, IoMemory, Lease, StagingLease},
    snapshot::SnapshotRequest,
    state::CommitOutcome,
};
use crate::scheduling::{IBlockingExecutor, IWorkHandle};
use pty_runtime_domain::{projection::ParkAttempt, terminal::TerminalCheckpoint};
use std::sync::{Arc, Mutex};

/// Where a blocking worker publishes the result of exactly one job.
///
/// `None` means still running. A panicking job publishes `Err(Worker)` rather
/// than leaving the slot empty, so a crashed worker can never look like one that
/// is merely slow.
pub(super) type Mailbox<T> = Arc<Mutex<Option<Result<T, ProjectionError>>>>;

/// Take a published result, treating an empty or poisoned slot as worker failure.
pub(super) fn collect<T>(mailbox: &Mailbox<T>) -> Result<T, ProjectionError> {
    mailbox
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
        .unwrap_or(Err(ProjectionError::Worker))
}

/// The single blocking job in flight, together with the state its completion
/// needs and the mailbox it will publish into.
///
/// The mailbox payload type is per-variant on purpose: a `Delete` cannot be
/// handed a checkpoint, and a `Restore` cannot be handed a commit outcome, so
/// the "wrong result kind" case that used to need a defensive arm in every
/// completion branch is now unrepresentable.
pub(super) enum PendingIo {
    Commit {
        attempt: ParkAttempt,
        disk: DiskLease,
        _memory: IoMemory,
        mailbox: Mailbox<CommitOutcome>,
    },
    Restore {
        resident: Lease,
        memory: IoMemory,
        mailbox: Mailbox<TerminalCheckpoint>,
    },
    Transfer {
        request: SnapshotRequest,
        memory: IoMemory,
        _staging: StagingLease,
        mailbox: Mailbox<TerminalCheckpoint>,
    },
    Delete {
        attempt: super::reaper::DeleteAttempt,
        mailbox: Mailbox<()>,
    },
}
impl PendingIo {
    /// Whether the blocking worker has published a result yet.
    fn is_ready(&self) -> bool {
        fn ready<T>(mailbox: &Mailbox<T>) -> bool {
            mailbox.lock().unwrap_or_else(|e| e.into_inner()).is_some()
        }
        match self {
            Self::Commit { mailbox, .. } => ready(mailbox),
            Self::Restore { mailbox, .. } => ready(mailbox),
            Self::Transfer { mailbox, .. } => ready(mailbox),
            Self::Delete { mailbox, .. } => ready(mailbox),
        }
    }
}

/// At most one outstanding blocking job.
#[derive(Default)]
pub(super) struct InFlight(Option<PendingIo>);

impl InFlight {
    /// Whether a job is outstanding. Callers use this to decide whether they may
    /// start one, and the worker uses it to decide it has nothing left to do.
    pub fn busy(&self) -> bool {
        self.0.is_some()
    }

    /// Take the job only if its worker has published a result.
    ///
    /// Leaving an unfinished job in place is the point: the slot is what stops a
    /// second job being started, and a job is not finished until its mailbox is
    /// filled — which a panicking worker also does.
    pub fn take_finished(&mut self) -> Option<PendingIo> {
        if !self.0.as_ref().is_some_and(PendingIo::is_ready) {
            return None;
        }
        self.0.take()
    }

    /// Abandon whatever is outstanding, finished or not. Only shutdown does
    /// this, and only to surrender the resources the job was holding.
    pub fn abandon(&mut self) -> Option<PendingIo> {
        self.0.take()
    }

    /// Submit one job and record it in the same step.
    ///
    /// `held` is whatever the completion will need and the caller is giving up —
    /// leases, an attempt, a waiting request — and `own` combines it with the
    /// mailbox into the variant that will own the job. There is no way to obtain
    /// a mailbox without filling the slot, so a job cannot be left running with
    /// nothing recording it.
    ///
    /// A rejected submission — a full pool, or a slot that is already occupied —
    /// leaves the slot untouched and hands `held` straight back with the error,
    /// so the caller cannot forget that it still owns whatever it was about to
    /// pass on.
    ///
    /// **The two steps are not atomic, and do not need to be.** The executor may
    /// run the job, fill the mailbox and wake the projection before `self.0` is
    /// assigned. What makes that safe is the workspace lock: the caller reaches
    /// this method holding it, and the woken run cannot look at the slot until
    /// the submitting run has released it. Anything that moved a submission out
    /// from under that lock would have to make this assignment happen first.
    pub fn submit<T: Send + 'static, H>(
        &mut self,
        executor: &dyn IBlockingExecutor,
        wake: Option<Arc<dyn IWorkHandle>>,
        work: impl FnOnce() -> Result<T, ProjectionError> + Send + 'static,
        held: H,
        own: impl FnOnce(H, Mailbox<T>) -> PendingIo,
    ) -> Result<(), (H, ProjectionError)> {
        // Checked rather than asserted: a `debug_assert` is absent from release
        // builds, and overwriting the slot would drop a running job's `PendingIo`
        // — releasing its leases while the worker still holds them and losing a
        // commit attempt or a delete checkout. Every caller guards this already;
        // the point of the check is that the invariant belongs to the type, not
        // to the discipline of its callers.
        if self.0.is_some() {
            return Err((held, ProjectionError::Worker));
        }
        let mailbox = match blocking::submit(executor, wake, work) {
            Ok(mailbox) => mailbox,
            Err(error) => return Err((held, error)),
        };
        self.0 = Some(own(held, mailbox));
        Ok(())
    }
}

#[cfg(test)]
impl InFlight {
    /// What is outstanding, for tests asserting which job a run started.
    pub fn peek(&self) -> Option<&PendingIo> {
        self.0.as_ref()
    }
}
