//! Authoritative projection, independent lossless staging and encrypted idle parking.
mod admission;
mod budgets;
mod completion;
mod coordinator;
mod io;
mod native;
mod observation;
mod state;
mod teardown;
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
