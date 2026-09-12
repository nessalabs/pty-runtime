//! The deletion lifecycle, driven without a coordinator.
//!
//! Deleting a superseded source is bounded retrying over an injected provider:
//! check the oldest source out, submit a job for it, and put it back — with or
//! without spending an attempt — depending on what came back. That whole cycle
//! belongs to [`SourceReaper`], so it can be driven here directly.
//!
//! The distinction these tests exist for is between the two ways a delete does
//! not happen. A provider that answered "no" is evidence, and spends an attempt.
//! A blocking pool that was full never asked the provider anything, so it must
//! spend nothing — otherwise a busy runtime exhausts a source's retries without
//! the provider ever having refused, and its disk reservation is surrendered for
//! the runtime's life over a transient queue depth.
use super::support::{Jobs, Store};
use crate::projection::{
    ProjectionBudgets, ProjectionError,
    budgets::DiskLease,
    inflight::{FinishedIo, InFlight},
    reaper::SourceReaper,
    state::CommittedSource,
};
use crate::scheduling::WorkSchedule;
use pty_runtime_domain::{
    ReplayCursor, SessionLifetime,
    checkpoint::{CheckpointError, CheckpointKey, CheckpointRef},
    projection::ProjectionLimits,
    terminal::{CheckpointDescriptor, CompatibilityId, ControlGeneration},
};
use std::sync::{Arc, atomic::Ordering};

fn source(budgets: &ProjectionBudgets, id: u8) -> CommittedSource {
    let key = CheckpointKey {
        lifetime: SessionLifetime::new(4, 1),
        generation: 1,
    };
    let mut object_id = [0; 16];
    object_id[0] = id;
    CommittedSource {
        reference: CheckpointRef {
            key,
            bytes: 32,
            object_id,
        },
        descriptor: CheckpointDescriptor {
            compatibility: CompatibilityId::new("reaper-fixture").unwrap(),
            processed: ReplayCursor {
                lifetime: key.lifetime,
                offset: 0,
            },
            control_generation: ControlGeneration::from_raw(0),
        },
        _disk: DiskLease::acquire(budgets, 32).unwrap(),
    }
}

struct Fixture {
    budgets: Arc<ProjectionBudgets>,
    store: Arc<Store>,
    jobs: Arc<Jobs>,
    reaper: SourceReaper,
    io: InFlight,
}
impl Fixture {
    fn new(max_attempts: u32) -> Self {
        Self {
            budgets: ProjectionBudgets::new(ProjectionLimits::default()).unwrap(),
            store: Arc::new(Store::default()),
            jobs: Arc::new(Jobs::default()),
            reaper: SourceReaper::new(max_attempts),
            io: InFlight::default(),
        }
    }
    fn retire(&mut self, id: u8) {
        let source = source(&self.budgets, id);
        self.reaper.retire(source);
    }
    fn start(&mut self) -> WorkSchedule {
        self.reaper
            .start_delete(&mut self.io, self.store.clone(), self.jobs.as_ref(), None)
    }
    /// Run the submitted job and settle it, as the worker would.
    fn settle(&mut self) -> Option<ProjectionError> {
        assert!(self.jobs.run_one(), "no job was submitted");
        let Some(FinishedIo::Delete { attempt, result }) = self.io.take_finished() else {
            panic!("a finished delete should be waiting in the slot");
        };
        self.reaper.finish_delete(attempt, result)
    }
}

/// A full blocking pool must not cost the source an attempt: nothing was asked
/// of the provider, so nothing was learned about it.
///
/// With `max_attempts` of 1, spending an attempt here would strand the source
/// permanently. The test therefore rejects a submission once, then lets the
/// retry through and requires the delete to actually reach the store.
#[test]
fn a_rejected_submission_costs_no_attempt_and_the_retry_still_deletes() {
    let mut f = Fixture::new(1);
    f.retire(1);
    f.jobs.reject.store(true, Ordering::Release);

    let schedule = f.start();
    assert!(
        matches!(schedule, WorkSchedule::After(delay) if !delay.is_zero()),
        "a rejected submission should ask to be retried shortly"
    );
    assert_eq!(f.jobs.len(), 0, "nothing should have reached the pool");
    assert!(!f.io.busy(), "the slot must stay free for another job");
    assert!(f.reaper.holds_sources());

    f.jobs.reject.store(false, Ordering::Release);
    assert!(matches!(f.start(), WorkSchedule::Dormant));
    assert!(f.io.busy(), "an accepted submission fills the slot");
    assert_eq!(f.settle(), None);
    assert_eq!(f.store.deletes.load(Ordering::Acquire), 1);
    assert!(
        !f.reaper.holds_sources(),
        "a successful delete drops the source and releases its reservation"
    );
}

/// A provider that refuses spends an attempt, and the failure only becomes a
/// durable outcome once the last one is spent.
#[test]
fn a_refusing_provider_spends_attempts_and_reports_only_when_they_run_out() {
    let mut f = Fixture::new(2);
    f.retire(1);
    f.store.fail_delete.store(true, Ordering::Release);

    f.start();
    assert_eq!(f.settle(), None, "one attempt left, so not yet durable");
    f.start();
    assert_eq!(
        f.settle(),
        Some(ProjectionError::Storage(CheckpointError::Unavailable)),
        "the last spent attempt is what makes the failure durable"
    );
    assert_eq!(f.store.deletes.load(Ordering::Acquire), 2);

    // Spent retries leave the source held, which is what stops the projection
    // going on to create more storage it cannot reclaim.
    assert!(f.reaper.holds_sources());
    assert!(
        matches!(f.start(), WorkSchedule::Dormant),
        "a source with no attempts left is not checked out again"
    );
    assert_eq!(f.jobs.len(), 0);

    // An explicit close asks for one more full round.
    f.reaper.restore_attempts();
    f.store.fail_delete.store(false, Ordering::Release);
    f.start();
    assert_eq!(f.settle(), None);
    assert!(!f.reaper.holds_sources());
}

/// The oldest source goes first, and a stuck head is not skipped over: deleting
/// a newer source while an older one is stuck would reorder the provider's view
/// of this session's storage.
#[test]
fn a_stuck_oldest_source_blocks_the_newer_one_behind_it() {
    let mut f = Fixture::new(1);
    f.retire(1);
    f.retire(2);
    f.store.fail_delete.store(true, Ordering::Release);

    f.start();
    assert!(f.settle().is_some(), "the head has spent its only attempt");
    assert!(
        matches!(f.start(), WorkSchedule::Dormant),
        "the newer source must not be deleted around the stuck one"
    );
    assert_eq!(f.jobs.len(), 0);
    assert_eq!(f.store.deletes.load(Ordering::Acquire), 1);
}

/// The one-job-at-a-time rule belongs to the slot, not to its callers.
///
/// Every caller checks the slot before asking for a deletion, so this path is
/// not reached in the worker as it stands. It is reachable here because
/// `InFlight` is a type that can be driven on its own — which is the point of it
/// being one. Overwriting an occupied slot would drop a running job's
/// `PendingIo`, releasing leases the blocking worker is still using and losing
/// whatever its completion was going to settle.
#[test]
fn a_second_deletion_is_refused_while_one_is_outstanding() {
    let mut f = Fixture::new(2);
    f.retire(1);
    f.retire(2);

    assert!(matches!(f.start(), WorkSchedule::Dormant));
    assert!(f.io.busy());
    assert_eq!(f.jobs.len(), 1);

    let schedule = f.start();
    assert!(
        matches!(schedule, WorkSchedule::After(delay) if !delay.is_zero()),
        "a refused submission should ask to be retried shortly"
    );
    assert_eq!(f.jobs.len(), 1, "the second job must not reach the pool");
    assert!(f.io.busy(), "the outstanding job must still own the slot");

    // The first completes normally, and the second then goes through with its
    // attempt count untouched by the refusal.
    assert_eq!(f.settle(), None);
    assert_eq!(f.store.deletes.load(Ordering::Acquire), 1);
    assert!(matches!(f.start(), WorkSchedule::Dormant));
    assert_eq!(f.settle(), None);
    assert_eq!(f.store.deletes.load(Ordering::Acquire), 2);
    assert!(!f.reaper.holds_sources());
}
