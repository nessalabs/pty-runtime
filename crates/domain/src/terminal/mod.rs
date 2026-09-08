//! Engine-neutral terminal projection contracts and resource limits.
mod checkpoint;
mod view;
pub use checkpoint::*;
pub use view::*;

/// Validated character-cell dimensions shared by process and terminal ports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalSize {
    cols: u16,
    rows: u16,
}
impl TerminalSize {
    /// Reject zero dimensions and grids exceeding 1,048,576 cells before allocation.
    pub fn new(cols: u16, rows: u16) -> Result<Self, TerminalError> {
        if cols == 0 || rows == 0 || u32::from(cols) * u32::from(rows) > 1_048_576 {
            return Err(TerminalError::InvalidConfiguration);
        }
        Ok(Self { cols, rows })
    }
    /// Width in character cells.
    pub fn cols(self) -> u16 {
        self.cols
    }
    /// Height in character cells.
    pub fn rows(self) -> u16 {
        self.rows
    }
}

/// Explicit bounds; native history accounting can round to page granularity.
#[derive(Debug, Clone, Copy)]
pub struct TerminalConfig {
    /// Initial dimensions; restored checkpoints must fit the cell admission bound.
    pub size: TerminalSize,
    /// Native scrollback target, excluding active screen and page overhead.
    pub history_bytes: usize,
    /// Maximum tracked unfinished parser input.
    pub continuation_bytes: usize,
    /// Maximum generated reply bytes per admitted feed operation.
    pub reply_bytes: usize,
    /// Maximum encoded checkpoint bytes.
    pub checkpoint_bytes: usize,
    /// Hard cap on requested native allocation bytes, excluding allocator overhead.
    pub native_bytes: usize,
    /// Maximum copied cell text bytes in one view.
    pub view_bytes: usize,
    /// Maximum input chunk accepted in one operation.
    pub feed_bytes: usize,
}
impl TerminalConfig {
    /// Bounded defaults for an authoritative terminal at the supplied dimensions.
    /// Native reservation is eight MiB; history targets one MiB plus native overhead.
    pub fn new(size: TerminalSize) -> Self {
        Self {
            size,
            history_bytes: 1024 * 1024,
            continuation_bytes: 65536,
            reply_bytes: 4096,
            checkpoint_bytes: 8 * 1024 * 1024,
            native_bytes: 8 * 1024 * 1024,
            view_bytes: 1024 * 1024,
            feed_bytes: 4096,
        }
    }
    /// Validate allocation limits before constructing an engine.
    pub fn validate(self) -> Result<Self, TerminalError> {
        if self.native_bytes == 0
            || self.native_bytes > 1024 * 1024 * 1024
            || self.view_bytes == 0
            || self.view_bytes > 64 * 1024 * 1024
            || self.history_bytes > 256 * 1024 * 1024
            || self.continuation_bytes == 0
            || self.continuation_bytes > 1024 * 1024
            || self.reply_bytes == 0
            || self.reply_bytes > 1024 * 1024
            || self.checkpoint_bytes == 0
            || self.checkpoint_bytes > 512 * 1024 * 1024
            || self.feed_bytes == 0
            || self.feed_bytes > 1024 * 1024
        {
            return Err(TerminalError::InvalidConfiguration);
        }
        Ok(self)
    }
}

/// Stable, redacted terminal failures without native codes or payload data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalError {
    /// Invalid dimensions or allocation policy.
    InvalidConfiguration,
    /// A configured buffer bound was exceeded.
    BudgetExceeded,
    /// Native allocation or processing failed; projection must be treated as failed.
    EngineFailure,
    /// Checkpoint bytes are invalid or incomplete.
    CorruptCheckpoint,
    /// Checkpoint compatibility identity does not match this adapter.
    IncompatibleCheckpoint,
    /// Ordered control generation did not advance exactly once.
    StaleControl,
    /// An operation requires all history to be restored first.
    HistoryIncomplete,
    /// Requested optional capability is unavailable.
    Unsupported,
}

/// Engine behavior advertised through portable capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalCapabilities {
    /// Full parser continuation and binary terminal state are retained.
    pub checkpoints: bool,
    /// Usable state may precede history completion.
    pub incremental_restore: bool,
    /// Live mutation can preserve complete history before restoration finishes.
    pub mutation_during_restore: bool,
    /// The engine supports resident history compression.
    pub history_compression: bool,
}

/// A bounded authoritative reply batch; Debug never reveals terminal contents.
#[derive(Clone, PartialEq, Eq)]
pub struct TerminalEffects(pub Vec<u8>);
impl std::fmt::Debug for TerminalEffects {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalEffects")
            .field("bytes", &self.0.len())
            .finish()
    }
}
