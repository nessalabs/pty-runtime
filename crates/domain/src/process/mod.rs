//! Portable command and process lifecycle models.
mod command;
mod outcome;
pub use command::{CommandSpec, EnvironmentPolicy};
pub use outcome::{DrainOutcome, ExitStatus, ProcessError, ProcessLimits, WriteOutcome};
#[cfg(test)]
mod tests;
