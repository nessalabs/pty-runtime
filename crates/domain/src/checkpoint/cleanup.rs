use super::CheckpointError;

/// Finite work admitted for one abandoned-checkpoint maintenance pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckpointCleanupLimits {
    /// Maximum candidate namespaces whose ownership may be inspected.
    pub namespaces: usize,
    /// Maximum directory entries inspected, including live/unknown namespaces.
    pub entries: usize,
    /// Maximum logical object bytes unlinked during this pass.
    pub bytes: u64,
}
impl Default for CheckpointCleanupLimits {
    fn default() -> Self {
        Self {
            namespaces: 256,
            entries: 4096,
            bytes: 1024 * 1024 * 1024,
        }
    }
}
impl CheckpointCleanupLimits {
    /// Reject zero or unbounded work before opening a maintenance namespace.
    pub fn validate(self) -> Result<(), CheckpointError> {
        if self.namespaces == 0
            || self.namespaces > 65536
            || self.entries == 0
            || self.entries > 1_000_000
            || self.bytes == 0
            || self.bytes > 1_u64 << 50
        {
            return Err(CheckpointError::InvalidConfiguration);
        }
        Ok(())
    }
}

/// Observed maintenance work, never an estimate of unexamined abandoned bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CheckpointCleanupReport {
    /// Every inspected entry, including unknown names and live owners.
    pub examined_entries: usize,
    /// Candidate namespaces inspected in this pass.
    pub examined_namespaces: usize,
    /// Namespaces skipped because their live owner still holds its lock.
    pub live_namespaces: usize,
    /// Abandoned namespaces completely removed.
    pub reclaimed_namespaces: usize,
    /// Validated protected or partial objects removed.
    pub reclaimed_objects: usize,
    /// Logical bytes in those objects; excludes filesystem allocation overhead.
    pub reclaimed_bytes: u64,
    /// Candidates with validation or filesystem errors, excluding budget stops.
    pub failed_namespaces: usize,
    /// The arena enumeration reached its end within the scan budget.
    pub scan_complete: bool,
    /// A positively identified abandoned namespace still needs cleanup.
    pub abandoned_incomplete: bool,
    /// First redacted validation/removal error; successful work remains reported.
    pub failure: Option<CheckpointError>,
}
