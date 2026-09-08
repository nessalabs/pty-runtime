//! Bounded Unix PTY session ownership and replaceable headless terminal projection.
//!
//! The session runtime is under implementation. Raw session contracts and native
//! terminal contracts have distinct tests; see the ADR proof ledger for scope.
#![forbid(unsafe_code)]
mod adapters;
mod runtime;
pub use adapters::StorageOptions;
pub use pty_runtime_application::projection::{
    PinnedCheckpoint, ProjectedView, ProjectionOperation, ProjectionServices,
};
pub use pty_runtime_application::runtime::{
    AttachPosition, Attachment, Completion, CompletionWait, NextOutput, OutputEvent, RuntimeError,
    RuntimeOptions, Session, SessionOptions, SessionStatus,
};
/// Engine-neutral encrypted checkpoint provider models.
pub use pty_runtime_domain::checkpoint;
pub use pty_runtime_domain::process::{
    CommandSpec, DrainOutcome, EnvironmentPolicy, ExitStatus, ProcessError, ProcessLimits,
    WriteOutcome,
};
pub use pty_runtime_domain::projection::{
    ProjectionError, ProjectionLimits, ProjectionOptions, ProjectionStatus, Residency,
    ResizeOutcome,
};
/// Engine-neutral terminal types for injected engines and headless consumers.
pub use pty_runtime_domain::terminal;
pub use pty_runtime_domain::terminal::TerminalSize;
pub use pty_runtime_domain::{ReplayCursor, ReplayPage, SessionId, SessionLifetime};
pub use runtime::Runtime;
/// Replaceable application boundaries for custom embedding infrastructure.
pub mod ports {
    pub use pty_runtime_application::checkpoint::{ICheckpointProtector, ICheckpointStore};
    pub use pty_runtime_application::process::{
        IInputReservation, IProcessBackend, IProcessEvents, IProcessSession, OutputAcceptance,
        ProcessOperation,
    };
    pub use pty_runtime_application::runtime::{ISessionRepository, SessionContext};
    pub use pty_runtime_application::scheduling::{
        IBlockingExecutor, ICapacitySignal, IClock, IScheduledWork, IWorkHandle, IWorkScheduler,
        SchedulingError, WorkSchedule,
    };
    pub use pty_runtime_application::terminal::{ITerminal, ITerminalFactory};
}
