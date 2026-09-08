//! Replaceable process ports. Adapters own OS handles, workers and synchronization.
use pty_runtime_domain::{
    SessionLifetime,
    process::{CommandSpec, DrainOutcome, ExitStatus, ProcessError, ProcessLimits, WriteOutcome},
    terminal::TerminalSize,
};
use std::{future::Future, pin::Pin, sync::Arc};

/// An already-admitted asynchronous operation. Dropping a wait does not undo admission.
pub type ProcessOperation<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

/// Result of bounded lossless output admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputAcceptance {
    /// The entire offered chunk was accepted exactly once.
    Accepted,
    /// Nothing was accepted. Retain the same chunk and wait for capacity.
    Backpressure,
    /// Session has been removed; delivery must stop.
    Closed,
}

/// Process-to-application events translated into domain types.
/// Implementations must not synchronously call back into the process handle.
pub trait IProcessEvents: Send + Sync {
    /// Atomically accept all bytes or none; replay eviction is not parser loss.
    fn output(&self, bytes: &[u8]) -> OutputAcceptance;
    /// Wait for capacity or termination without polling; bounded by the adapter's shutdown policy.
    fn wait_for_capacity(&self, deadline: std::time::Instant);
    /// Report actual child exit independently from reader completion.
    fn exited(&self, status: ExitStatus);
    /// Report final drain result exactly once.
    fn drained(&self, outcome: DrainOutcome);
    /// Report supervision failure without manufacturing child exit.
    fn supervision_failed(&self, error: ProcessError);
}

/// A runtime admission lease transferred to the actual queued input owner.
/// The adapter drops it only after clearing/releasing the input, even if its waiter is dropped.
pub trait IInputReservation: Send {}

/// One owned process lifetime; dropping observer/session handles does not kill it.
pub trait IProcessSession: Send + Sync {
    /// Diagnostic child ID. Only the adapter may use it for synchronized controls.
    fn process_id(&self) -> u32;
    /// Reserve chunk/byte admission before copying input; a rejected call copies nothing.
    fn write(&self, bytes: &[u8]) -> Result<ProcessOperation<WriteOutcome>, ProcessError> {
        self.write_reserved(bytes, None)
    }
    /// Transfer an external global reservation into the input queue, before copying bytes.
    /// Failure drops the lease. Completion drops it independently of caller wait lifetime.
    fn write_reserved(
        &self,
        bytes: &[u8],
        reservation: Option<Box<dyn IInputReservation>>,
    ) -> Result<ProcessOperation<WriteOutcome>, ProcessError>;
    /// Admit one coalesced cancellation sequence independently of the input queue.
    fn request_cancel(&self) -> Result<(), ProcessError>;
    /// Resize OS state, reporting the actual operation result.
    fn resize(
        &self,
        size: TerminalSize,
    ) -> Result<ProcessOperation<Result<(), ProcessError>>, ProcessError>;
}

/// Backend lifetime owns shared supervision and all its admitted processes.
pub trait IProcessBackend: Send + Sync {
    /// Allocate/spawn once. Early callbacks may occur; caller reserves registration first.
    /// A failed spawn must release partial descriptors/workers and reap any owned child.
    fn spawn(
        &self,
        command: &CommandSpec,
        size: TerminalSize,
        lifetime: SessionLifetime,
        limits: ProcessLimits,
        events: Arc<dyn IProcessEvents>,
    ) -> Result<Arc<dyn IProcessSession>, ProcessError>;
    /// Reject new spawns and terminate/reap owned processes. Runs independently of observers.
    fn shutdown(&self);
    /// Immediately kill/reap owned children for owner Drop, without the graceful delay.
    fn shutdown_now(&self);
}
