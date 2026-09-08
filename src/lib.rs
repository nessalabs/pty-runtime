//! Bounded Unix PTY session ownership and replaceable headless terminal projection.
//!
//! The session runtime is under implementation. Raw session contracts and native
//! terminal contracts have distinct tests; see the ADR proof ledger for scope.
#![forbid(unsafe_code)]
mod runtime;
pub use pty_runtime_application::runtime::{
    AttachPosition, Attachment, Completion, CompletionWait, NextOutput, OutputEvent, RuntimeError,
    RuntimeOptions, Session, SessionOptions, SessionStatus,
};
pub use pty_runtime_domain::process::{
    CommandSpec, DrainOutcome, EnvironmentPolicy, ExitStatus, ProcessError, ProcessLimits,
    WriteOutcome,
};
/// Engine-neutral terminal types for injected engines and headless consumers.
pub use pty_runtime_domain::terminal;
pub use pty_runtime_domain::terminal::TerminalSize;
pub use pty_runtime_domain::{ReplayCursor, ReplayPage, SessionId, SessionLifetime};
pub use runtime::Runtime;
/// Replaceable application boundaries for custom embedding infrastructure.
pub mod ports {
    pub use pty_runtime_application::process::{
        IInputReservation, IProcessBackend, IProcessEvents, IProcessSession, OutputAcceptance,
        ProcessOperation,
    };
    pub use pty_runtime_application::runtime::{ISessionRepository, SessionContext};
    pub use pty_runtime_application::terminal::{ITerminal, ITerminalFactory};
}
