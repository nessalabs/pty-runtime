use super::ControlGeneration;
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

/// Opaque engine and binary-format identity carried by every checkpoint.
///
/// Non-empty and at most [`Self::MAX_BYTES`], validated once here. The same rule
/// used to be re-derived independently by the projection's admission check, the
/// protector's authenticated metadata, and the checkpoint store's accepted-object
/// check; holding it in the type keeps those three from drifting apart and stops
/// an adapter that skips one of those paths from introducing an unbounded
/// identity.
///
/// The content is opaque. Only equality is meaningful, and only to the adapter
/// that produced it.
#[derive(Clone, PartialEq, Eq)]
pub struct CompatibilityId(Box<str>);

impl CompatibilityId {
    /// Longest accepted identity, in bytes.
    pub const MAX_BYTES: usize = 4096;

    /// Validate a nonempty identity within the byte bound.
    pub fn new(value: &str) -> Result<Self, super::TerminalError> {
        if value.is_empty() || value.len() > Self::MAX_BYTES {
            return Err(super::TerminalError::InvalidConfiguration);
        }
        Ok(Self(value.into()))
    }

    /// Borrow the identity for comparison or authenticated metadata.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for CompatibilityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompatibilityId")
            .field("bytes", &self.0.len())
            .finish()
    }
}

/// Ordering and compatibility metadata kept separate from opaque encoded bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointDescriptor {
    /// Opaque engine and binary-format identity interpreted only by its adapter.
    pub compatibility: CompatibilityId,
    /// Next output byte after all bytes already applied to the model.
    pub processed: ReplayCursor,
    /// Last successfully applied ordered control generation.
    pub control_generation: ControlGeneration,
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
