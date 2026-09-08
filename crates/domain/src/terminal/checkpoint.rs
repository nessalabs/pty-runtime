use crate::identity::ReplayCursor;

/// Engine-neutral restoration milestone; usable state is not complete history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestorationProgress {
    /// Active screens are usable; older history remains to be decoded.
    Usable,
    /// Every history unit from this source was validated and applied.
    Complete,
    /// Active state is usable, but live mutation made some source history inapplicable.
    UsableWithSkippedHistory {
        /// Validated history pages that could no longer be applied, for this source.
        skipped_pages: u64,
    },
    /// Source validation finished, but this is not complete history restoration.
    FinishedWithSkippedHistory {
        /// Validated history pages that could no longer be applied, for this source.
        skipped_pages: u64,
    },
}

impl RestorationProgress {
    /// Whether the source has been fully consumed and validated, including skipped history.
    pub fn is_finished(self) -> bool {
        matches!(
            self,
            Self::Complete | Self::FinishedWithSkippedHistory { .. }
        )
    }
    /// Number of history pages validated but not applied in this restoration.
    pub fn skipped_pages(self) -> u64 {
        match self {
            Self::Usable | Self::Complete => 0,
            Self::UsableWithSkippedHistory { skipped_pages }
            | Self::FinishedWithSkippedHistory { skipped_pages } => skipped_pages,
        }
    }
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
