use crate::identity::ReplayCursor;

/// Engine-neutral restoration milestone; usable state is not complete history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestorationProgress {
    /// Active screens are observable; live mutation requires the engine capability
    /// or completion of retained history.
    Usable,
    /// Retained history has also completed.
    Complete,
}

/// Ordering and compatibility metadata kept separate from opaque encoded bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointDescriptor {
    /// Opaque engine and binary-format identity interpreted only by its adapter.
    pub compatibility: String,
    /// Next output byte after all bytes already applied to the model.
    pub processed: ReplayCursor,
    /// Last successfully applied ordered control generation.
    pub control_generation: u64,
}

/// Owned opaque terminal state, for encryption before any storage port receives it.
#[derive(Clone)]
pub struct TerminalCheckpoint {
    /// Compatibility and logical stream ordering.
    pub descriptor: CheckpointDescriptor,
    /// Adapter-specific binary state; consumers must not interpret this payload.
    pub bytes: Vec<u8>,
}
impl std::fmt::Debug for TerminalCheckpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalCheckpoint")
            .field("bytes", &self.bytes.len())
            .finish()
    }
}
impl Drop for TerminalCheckpoint {
    fn drop(&mut self) {
        self.bytes.fill(0);
    }
}
