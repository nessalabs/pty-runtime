//! Replaceable terminal engine boundary. Implementations own native resources.
use pty_runtime_domain::terminal::{
    CheckpointDescriptor, RestorationProgress, TerminalCapabilities, TerminalCheckpoint,
    TerminalConfig, TerminalEffects, TerminalError, TerminalSize, TerminalView,
};

/// Creates exclusive terminal owners; implementations reject unsupported contracts.
pub trait ITerminalFactory: Send + Sync {
    /// Features guaranteed by terminals made by this factory.
    fn capabilities(&self) -> TerminalCapabilities;
    /// Opaque binary compatibility identity for checkpoint descriptors.
    fn compatibility(&self) -> &'static str;
    /// Validate limits before native allocation; no stub or empty fallback on failure.
    fn create(&self, config: TerminalConfig) -> Result<Box<dyn ITerminal>, TerminalError>;
    /// Restore usable active state. Consumes and retains bounded bytes until history completes.
    /// Failure preserves caller-owned process state; no process operation occurs here.
    fn restore(
        &self,
        checkpoint: TerminalCheckpoint,
        config: TerminalConfig,
    ) -> Result<Box<dyn ITerminal>, TerminalError>;
}

/// Exclusive terminal owner. Calls are serialized by its application owner.
/// No operation can be cancelled midway; admission bounds their input/output resources.
pub trait ITerminal: Send {
    /// Feed one bounded ordered chunk; replies must enter the authoritative writer exactly once.
    /// HistoryIncomplete rejects the chunk unchanged; stage it until complete and retry.
    /// Other processing errors invalidate projection; retrying applied bytes is not safe.
    fn feed(&mut self, bytes: &[u8]) -> Result<TerminalEffects, TerminalError>;
    /// Apply one ordered resize, requiring generation exactly one above the previous control.
    /// HistoryIncomplete leaves control unapplied; finish restoration before retrying.
    fn resize(&mut self, size: TerminalSize, generation: u64) -> Result<(), TerminalError>;
    /// Copy active cells into domain values with no references to mutable engine memory.
    fn view(&mut self) -> Result<TerminalView, TerminalError>;
    /// Encode complete state under its byte cap; descriptor control must match the model.
    /// Application provides processed position at the same serialized boundary as feed.
    fn checkpoint(
        &mut self,
        descriptor: CheckpointDescriptor,
    ) -> Result<TerminalCheckpoint, TerminalError>;
    /// Current restoration milestone.
    fn restoration_progress(&self) -> RestorationProgress;
    /// Restore at most one native history unit, allowing observations between steps.
    /// Engines may reject mutations until Complete to preserve complete retained history.
    fn restore_history_step(&mut self) -> Result<RestorationProgress, TerminalError>;
    /// Perform one bounded unit of optional resident-history compression.
    /// Returns true if no further work remains.
    fn compress_history_step(&mut self) -> Result<bool, TerminalError>;
}
