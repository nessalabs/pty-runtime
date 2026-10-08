//! Infrastructure layer for the PTY runtime.

#[path = "../../atomic_update.rs"]
mod atomic;

pub mod identity;
pub mod process;
pub mod registry;
#[cfg(feature = "ghostty")]
pub mod terminal;

pub mod checkpoint;

pub mod scheduling;

#[cfg(feature = "event-stream")]
pub mod event_stream;
