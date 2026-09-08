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

pub(super) enum Event {
    Output(Vec<u8>, StagingLease),
    Resize(TerminalSize, Arc<Ticket<ResizeOutcome>>, StagingLease),
    View(Arc<Ticket<ProjectedView>>, StagingLease),
    Checkpoint(SnapshotRequest, StagingLease),
}
impl Event {
    pub fn fail(self, error: ProjectionError) {
        match self {
            Self::Resize(_, ticket, _) => ticket.complete(Err(error)),
            Self::View(ticket, _) => ticket.complete(Err(error)),
            Self::Checkpoint(request, _) => request.fail(error),
            Self::Output(_, _) => (),
        }
    }
}
pub(super) struct Core {
    pub policy: ProjectionPolicy,
    pub output_drain: Option<pty_runtime_domain::process::DrainOutcome>,
    pub queue: VecDeque<Event>,
    pub close_waiters: Vec<Arc<Ticket<()>>>,
    pub unreclaimed_failure: Option<ProjectionError>,
    pub cleanup_failure: Option<ProjectionError>,
    pub retry_cleanup: bool,
}
pub(super) struct Stored {
    pub reference: CheckpointRef,
    pub descriptor: CheckpointDescriptor,
    pub _disk: DiskLease,
}
pub(super) struct Unreclaimed {
    pub _reference: Option<CheckpointRef>,
    pub _key: pty_runtime_domain::checkpoint::CheckpointKey,
    pub _descriptor: CheckpointDescriptor,
    pub _disk: DiskLease,
}
impl From<Stored> for Unreclaimed {
    fn from(source: Stored) -> Self {
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
pub(super) enum IoKind {
    Commit {
        attempt: ParkAttempt,
        disk: DiskLease,
        _memory: IoMemory,
    },
    Restore {
        resident: Lease,
        memory: IoMemory,
    },
    Transfer {
        request: SnapshotRequest,
        memory: IoMemory,
        _staging: StagingLease,
    },
    Delete {
        source: Stored,
        attempts: u32,
    },
}
pub(super) enum IoResult {
    Failure,
    Commit(CommitOutcome),
    Read(Result<TerminalCheckpoint, ProjectionError>),
    Delete(Result<(), ProjectionError>),
}
pub(super) type IoMailbox = Arc<Mutex<Option<IoResult>>>;
pub(super) struct PendingIo {
    pub kind: IoKind,
    pub mailbox: IoMailbox,
}
pub(super) struct Engine {
    pub config: pty_runtime_domain::terminal::TerminalConfig,
    pub history_due: bool,
    pub terminal: Option<Box<dyn ITerminal>>,
    pub resident: Option<Lease>,
    pub source: Option<Stored>,
    pub restore_memory: Option<Lease>,
    pub io: Option<PendingIo>,
    pub reply: Option<Reply>,
    pub resize: Option<Resizing>,
    pub garbage: VecDeque<(Stored, u32)>,
}
