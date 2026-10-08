//! Application layer for the PTY runtime.
#![forbid(unsafe_code)]

#[path = "../../atomic_update.rs"]
mod atomic;

pub mod process;
pub mod runtime;
pub mod terminal;

pub mod checkpoint;
pub mod scheduling;

pub mod projection;

/// Explicit bounded internal latency measurements for release qualification.
pub mod diagnostics;
