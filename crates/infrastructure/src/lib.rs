//! Infrastructure layer for the PTY runtime.

pub mod identity;
pub mod process;
pub mod registry;
#[cfg(feature = "ghostty")]
pub mod terminal;

pub mod checkpoint;

pub mod scheduling;
