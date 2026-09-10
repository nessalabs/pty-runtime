use super::{
    budgets::{DiskLease, IoMemory, Lease, StagingLease},
    observation::{ProjectedView, Ticket},
    snapshot::SnapshotRequest,
};
use crate::{process::ProcessOperation, terminal::ITerminal};
use pty_runtime_domain::{
    checkpoint::CheckpointRef,
    projection::{ParkAttempt, ProjectionError, ProjectionPolicy, ResizeOutcome},
    terminal::{CheckpointDescriptor, TerminalCheckpoint, TerminalSize},
};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

/// One admitted unit of work waiting in the ordered queue.
///
/// These are requests going *in*, not facts coming out — the observable facts a
/// consumer reads back are `TransferEvent`s produced by the journal after the
/// corresponding command succeeds. Every variant carries the `StagingLease` that
/// admitted it, so dropping a command releases its quota exactly once.
pub(super) enum Command {
    Output(Vec<u8>, StagingLease),
    Resize(TerminalSize, Arc<Ticket<ResizeOutcome>>, StagingLease),
    View(Arc<Ticket<ProjectedView>>, StagingLease),
    Checkpoint(SnapshotRequest, StagingLease),
}
impl Command {
    pub fn fail(self, error: ProjectionError) {
        match self {
            Self::Resize(_, ticket, _) => ticket.complete(Err(error)),
            Self::View(ticket, _) => ticket.complete(Err(error)),
            Self::Checkpoint(request, _) => request.fail(error),
            Self::Output(_, _) => (),
        }
    }
}
/// Everything guarded by the *admission* lock: what has been accepted, and what
/// the domain policy believes about it.
///
/// This is the half callers touch. It is deliberately separate from
/// [`NativeWorkspace`] so that admitting output, reading status, or requesting
/// cancellation never has to wait behind a native terminal operation:
///
/// ```text
///   caller ──▶ [admission lock] ──▶ queue ──┐
///                                           │ worker drains the queue while
///   worker ──▶ [workspace lock] ──▶ native ─┘ holding the workspace lock
/// ```
///
/// Lock order when both are needed is **workspace → admission**: the worker
/// takes the workspace lock for the whole run and reaches into admission inside
/// it (`worker::run`, `worker::failed`, `teardown::finish_after_shutdown` are
/// the only three sites that take workspace at all). Callers only ever take
/// admission, so they can never invert the order. Never hold either lock across
/// a blocking store or OS call.
pub(super) struct Admission {
    pub policy: ProjectionPolicy,
    pub output_drain: Option<pty_runtime_domain::process::DrainOutcome>,
    pub queue: VecDeque<Command>,
    pub close_waiters: Vec<Arc<Ticket<()>>>,
    pub unreclaimed_failure: Option<ProjectionError>,
    pub cleanup_failure: Option<ProjectionError>,
    pub retry_cleanup: bool,
}
pub(super) struct CommittedSource {
    pub reference: CheckpointRef,
    pub descriptor: CheckpointDescriptor,
    pub _disk: DiskLease,
}
/// A source we failed to delete, or whose commit outcome was never confirmed.
///
/// Every field is read-never: this type exists purely to keep its `DiskLease`
/// (and the identity that lease was charged against) alive, so the quota stays
/// charged for storage we can no longer prove we released. Dropping one is the
/// only thing that releases that charge, which is why the runtime holds these in
/// a ledger for its whole lifetime rather than discarding them.
pub(super) struct UnreclaimedSource {
    pub _reference: Option<CheckpointRef>,
    pub _key: pty_runtime_domain::checkpoint::CheckpointKey,
    pub _descriptor: CheckpointDescriptor,
    pub _disk: DiskLease,
}
impl From<CommittedSource> for UnreclaimedSource {
    fn from(source: CommittedSource) -> Self {
        Self {
            _reference: Some(source.reference),
            _key: source.reference.key,
            _descriptor: source.descriptor,
            _disk: source._disk,
        }
    }
}
pub(super) enum CommitOutcome {
    Published(CheckpointRef),
    Rejected(ProjectionError),
    Uncertain(ProjectionError),
}
pub(super) struct Reply {
    pub bytes: Vec<u8>,
    pub _memory: Lease,
    pub operation: Option<ProcessOperation<pty_runtime_domain::process::WriteOutcome>>,
}
impl Drop for Reply {
    fn drop(&mut self) {
        self.bytes.fill(0);
    }
}
pub(super) struct Resizing {
    pub size: TerminalSize,
    pub generation: u64,
    pub ticket: Arc<Ticket<ResizeOutcome>>,
    pub _staging: StagingLease,
    pub operation: ProcessOperation<Result<(), pty_runtime_domain::process::ProcessError>>,
}
/// Where a blocking worker publishes the result of exactly one job.
///
/// `None` means still running. A panicking job publishes `Err(Worker)` rather
/// than leaving the slot empty, so a crashed worker can never look like one that
/// is merely slow.
pub(super) type Mailbox<T> = Arc<Mutex<Option<Result<T, ProjectionError>>>>;

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
        source: CommittedSource,
        attempts: u32,
        mailbox: Mailbox<()>,
    },
}
impl PendingIo {
    /// Whether the blocking worker has published a result yet.
    pub fn is_ready(&self) -> bool {
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
/// Take a published result, treating an empty or poisoned slot as worker failure.
pub(super) fn collect<T>(mailbox: &Mailbox<T>) -> Result<T, ProjectionError> {
    mailbox
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
        .unwrap_or(Err(ProjectionError::Worker))
}
/// Everything guarded by the *workspace* lock: exclusive ownership of the native
/// terminal and of whatever operation is currently in flight against it.
///
/// Only the worker touches this. Holding it means "I am the one driving the
/// engine right now", which is what lets [`ITerminal`] be a `&mut self` trait
/// without any interior locking of its own. `terminal` is `None` while the model
/// is parked or being restored; `io` holds at most one outstanding blocking job.
pub(super) struct NativeWorkspace {
    pub config: pty_runtime_domain::terminal::TerminalConfig,
    pub history_step_owed: bool,
    pub terminal: Option<Box<dyn ITerminal>>,
    pub resident: Option<Lease>,
    pub source: Option<CommittedSource>,
    pub restore_memory: Option<Lease>,
    pub io: Option<PendingIo>,
    pub reply: Option<Reply>,
    pub resize: Option<Resizing>,
    pub pending_deletes: VecDeque<(CommittedSource, u32)>,
}
