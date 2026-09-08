use super::ProjectionError;
use crate::terminal::{TerminalCell, TerminalConfig};
use std::time::Duration;
/// Global admission bounds shared by every projected session in one runtime.
#[derive(Debug, Clone, Copy)]
pub struct ProjectionLimits {
    /// Parser payload bytes, independent from evictable raw replay.
    pub staging_bytes: usize,
    /// Parser chunks and ordered request queue entries.
    pub staging_slots: usize,
    /// Reserved native allocation caps of all resident/restoring models.
    pub resident_bytes: usize,
    /// Encoded/decoded buffers and immutable transfer pins.
    pub checkpoint_bytes: usize,
    /// Provider-independent ciphertext reservation for pending commits and retained sources.
    pub stored_bytes: usize,
    /// Maximum retained or unreclaimed immutable source identities.
    pub stored_slots: usize,
    /// Copied terminal cells/text and pending authoritative reply buffers.
    pub view_bytes: usize,
    /// Outstanding observation/control waits, including completed retained results.
    pub request_slots: usize,
}
impl Default for ProjectionLimits {
    fn default() -> Self {
        Self {
            staging_bytes: 64 * 1024 * 1024,
            staging_slots: 16384,
            resident_bytes: 1024 * 1024 * 1024,
            checkpoint_bytes: 512 * 1024 * 1024,
            stored_bytes: 1024 * 1024 * 1024,
            stored_slots: 4096,
            view_bytes: 128 * 1024 * 1024,
            request_slots: 4096,
        }
    }
}
impl ProjectionLimits {
    /// Reject zero or impossible allocation bounds before reserving any storage.
    pub fn validate(self) -> Result<Self, ProjectionError> {
        let limits = [
            self.staging_bytes,
            self.staging_slots,
            self.resident_bytes,
            self.checkpoint_bytes,
            self.stored_bytes,
            self.stored_slots,
            self.view_bytes,
            self.request_slots,
        ];
        if self.stored_slots > 65536 || limits.iter().any(|&v| v == 0 || v > isize::MAX as usize) {
            Err(ProjectionError::InvalidConfiguration)
        } else {
            Ok(self)
        }
    }
}
/// Per-session projection admission and bounded idle-parking retry policy.
#[derive(Debug, Clone, Copy)]
pub struct ProjectionOptions {
    /// Native and synchronous operation limits enforced by the terminal adapter.
    pub terminal: TerminalConfig,
    /// Lossless parser payload cap; must admit a complete native feed chunk.
    pub staging_bytes: usize,
    /// Parser/control queue slots, including currently executing work.
    pub staging_slots: usize,
    /// Outstanding view/checkpoint/resize waits including completed retained results.
    pub request_slots: usize,
    /// Idle mutation interval before automatic parking; defaults to sixty seconds.
    pub park_after: Duration,
    /// Delay between failed parking attempts.
    pub retry_after: Duration,
    /// Maximum failed parking attempts per activity generation.
    pub max_park_attempts: u32,
}
impl ProjectionOptions {
    /// Default parking is enabled. Input bytes alone do not reset this policy.
    pub fn new(terminal: TerminalConfig) -> Self {
        Self {
            terminal,
            staging_bytes: 2 * 1024 * 1024,
            staging_slots: 256,
            request_slots: 32,
            park_after: Duration::from_secs(60),
            retry_after: Duration::from_secs(5),
            max_park_attempts: 3,
        }
    }
    /// Validate all finite bounds and checked copied-view allocation accounting.
    pub fn validate(self) -> Result<Self, ProjectionError> {
        self.terminal.validate()?;
        if self.staging_bytes < self.terminal.feed_bytes
            || self.staging_bytes > isize::MAX as usize
            || self.staging_slots == 0
            || self.staging_slots > 1_048_576
            || self.request_slots == 0
            || self.request_slots > 1_048_576
            || self.park_after.is_zero()
            || self.retry_after.is_zero()
            || self.park_after > Duration::from_secs(86400 * 365)
            || self.retry_after > Duration::from_secs(86400 * 365)
            || self.max_park_attempts == 0
            || self.max_park_attempts > 100
        {
            return Err(ProjectionError::InvalidConfiguration);
        }
        self.view_reservation()?;
        Ok(self)
    }
    /// Native view text cap plus all admitted cell structs, excluding caller object metadata.
    pub fn view_reservation(self) -> Result<usize, ProjectionError> {
        (usize::from(self.terminal.size.cols()) * usize::from(self.terminal.size.rows()))
            .checked_mul(std::mem::size_of::<TerminalCell>())
            .and_then(|v| v.checked_add(self.terminal.view_bytes))
            .ok_or(ProjectionError::InvalidConfiguration)
    }
}
