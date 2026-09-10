//! Independent authoritative projection order, residency and parking policy.
mod options;
mod policy;
mod transfer;
use crate::{
    ReplayCursor, checkpoint::CheckpointError, process::ProcessError, terminal::TerminalError,
};
pub use options::{ProjectionLimits, ProjectionOptions};
pub use policy::{ParkAttempt, ProjectionPolicy};
pub use transfer::{TransferBoundary, TransferCursor, TransferEnd, TransferError, TransferOrder};

/// Terminal ownership state, independent from process exit and output drain.
///
/// Every transition below is a method on [`ProjectionPolicy`]. The application
/// serializes those calls and owns the external work each one authorizes.
///
/// ```text
///              ┌──────────── new() ────────────┐
///              ▼                               │
///        ┌───────────┐  begin_park   ┌─────────┴─┐
///        │ Resident  │──────────────▶│  Parking  │
///        │           │◀──────────────│           │
///        └───────────┘  park_failed  └─────┬─────┘
///              ▲        commit_park(✗)     │ commit_park(✓)
///              │                           ▼
///              │                     ┌───────────┐  live model released;
///              │                     │  Parked   │  the committed source is
///              │                     └─────┬─────┘  now authoritative
///              │                           │ begin_restore
///              │                           ▼
///              │  progress.is_finished()  ┌───────────┐
///              ├──────────────────────────│ Restoring │
///              │                          └─────┬─────┘
///              │                                │ restoration_progress (partial)
///              │  progress.is_finished()  ┌─────▼─────┐  active screens visible,
///              └──────────────────────────│  Usable   │  history still incomplete
///                                         └───────────┘
///
///   any state ── fail() ──▶ Failed ──┐
///   any state ─────────── close() ───┴──▶ Closing ── mark_closed() ──▶ Closed
/// ```
///
/// `Failed` is terminal apart from `close()`, and `begin_restore` refuses to
/// leave it. `close()` and `fail()` are both no-ops once `Closing`/`Closed` is
/// reached, so a late worker cannot revive a projection. `cleanup_failed` and
/// `maintenance_failed` record a failure *without* moving residency, which is
/// why a `Closed` projection can still carry one.
///
/// Output admission is deliberately not on this chart: `admit_output` rejects
/// only `Closing`/`Closed`, so bytes keep being staged while projection is
/// `Failed` and the raw replay stream stays independent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Residency {
    /// A live model is available; history applicability is reported separately.
    Resident,
    /// Immutable state is being encoded or committed; live model is retained.
    Parking,
    /// The committed immutable source is the authoritative model.
    Parked,
    /// Bounded source read/authentication/READY construction is in progress.
    Restoring,
    /// Active screens are observable, but retained history is not complete.
    Usable,
    /// Projection is unavailable; process and bounded raw output remain independent.
    Failed,
    /// Cleanup has been requested; no operation may revive the projection.
    Closing,
    /// All model and checkpoint ownership has been released.
    Closed,
}
/// Redacted projection failure with its original portable boundary category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionError {
    /// Finite memory or operation admission is exhausted.
    Capacity,
    /// Invalid bounds, compatibility or stream position.
    InvalidConfiguration,
    /// No further work is accepted.
    Closed,
    /// Native projection failed; bytes must not be retried against mutated state.
    Terminal(TerminalError),
    /// Opaque checkpoint protection/provider failed.
    Storage(CheckpointError),
    /// Authoritative input or ordered OS control failed.
    Process(ProcessError),
    /// Worker infrastructure or an injected callback failed.
    Worker,
}
/// One consistent snapshot of authoritative byte/control positions and residency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectionStatus {
    /// End of output admitted to lossless parser staging.
    pub published: ReplayCursor,
    /// End of output successfully applied to the model.
    pub processed: ReplayCursor,
    /// Last model resize applied successfully.
    pub control_generation: crate::terminal::ControlGeneration,
    /// Independent terminal residency/history state.
    pub residency: Residency,
    /// Current or most recent restoration, including explicitly inapplicable history.
    pub history: crate::terminal::RestorationProgress,
    /// Cumulative validated history pages that could not be applied in this lifetime.
    pub skipped_history_pages: u64,
    /// Permanent projection failure, if any.
    pub failure: Option<ProjectionError>,
    /// Latest recoverable parking failure; original live state is retained.
    pub parking_failure: Option<ProjectionError>,
}
/// Both sides of an ordered resize; an OS change is never concealed after model failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResizeOutcome {
    /// Attempted next ordered control generation.
    pub generation: crate::terminal::ControlGeneration,
    /// Whether the OS accepted the requested PTY dimensions.
    pub os: Result<(), ProcessError>,
    /// Whether the authoritative terminal accepted the same dimensions.
    pub model: Result<(), ProjectionError>,
}
impl From<TerminalError> for ProjectionError {
    fn from(e: TerminalError) -> Self {
        Self::Terminal(e)
    }
}
impl From<CheckpointError> for ProjectionError {
    fn from(e: CheckpointError) -> Self {
        Self::Storage(e)
    }
}
impl From<ProcessError> for ProjectionError {
    fn from(e: ProcessError) -> Self {
        Self::Process(e)
    }
}
#[cfg(test)]
mod tests;

#[cfg(test)]
mod transfer_tests;
