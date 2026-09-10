//! Authoritative projection, independent lossless staging and encrypted idle parking.
mod admission;
mod budgets;
mod completion;
mod coordinator;
mod io;
mod journal;
mod snapshot;
mod stream_end;
mod transfer;
pub use pty_runtime_domain::projection::{
    TransferBoundary, TransferCursor, TransferEnd, TransferError,
};
pub use transfer::{
    StateTransfer, TransferEvent, TransferEventKind, TransferObserver, TransferRead,
};
mod native;
mod observation;
mod reaper;
mod state;
mod teardown;
mod wiring;
mod worker;

pub use budgets::ProjectionBudgets;
pub use coordinator::{ProjectionCoordinator, ProjectionServices};
pub use observation::{PinnedCheckpoint, ProjectedView, ProjectionOperation};
pub use pty_runtime_domain::projection::{
    ProjectionError, ProjectionLimits, ProjectionOptions, ProjectionStatus, Residency,
    ResizeOutcome,
};
#[cfg(test)]
mod tests;
