//! Domain layer for the PTY runtime.
#![forbid(unsafe_code)]

mod identity;
mod replay;

pub use identity::{InvalidSessionId, ReplayCursor, SessionId, SessionLifetime};
pub use replay::{ReplayBuffer, ReplayError, ReplayPage};

pub mod process;
pub mod terminal;

pub mod session;

pub mod checkpoint;
