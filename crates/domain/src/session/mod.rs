//! Session policies, lifecycle facts and observation models.
use crate::{
    ReplayCursor, ReplayPage,
    process::{DrainOutcome, ExitStatus, ProcessError, ProcessLimits},
    projection::{ProjectionError, ProjectionLimits, ProjectionOptions},
    terminal::{TerminalConfig, TerminalSize},
};

/// Application failures do not expose registry locks or external errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeError {
    /// Registry ID is already reserved or retained.
    ExistingSession,
    /// No registry entry matches this ID.
    MissingSession,
    /// A bounded resource cannot admit this operation.
    Capacity,
    /// Runtime is shutting down or has failed.
    Closed,
    /// Explicit removal requires process and drain completion.
    NotFinished,
    /// A supplied cursor is foreign or ahead of this session.
    InvalidCursor,
    /// Process boundary failure.
    Process(ProcessError),
    /// Independent terminal admission or projection failure.
    Projection(ProjectionError),
    /// A core invariant or synchronization failed.
    Internal,
}
impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for RuntimeError {}
impl From<ProcessError> for RuntimeError {
    fn from(e: ProcessError) -> Self {
        Self::Process(e)
    }
}

impl From<ProjectionError> for RuntimeError {
    fn from(error: ProjectionError) -> Self {
        Self::Projection(error)
    }
}

/// Global application admission ceilings. All remain finite.
#[derive(Debug, Clone)]
pub struct RuntimeOptions {
    /// Registered sessions, including completed entries awaiting explicit removal.
    pub max_sessions: usize,
    /// Attachments and completion waits combined across the runtime.
    pub max_observers: usize,
    /// Reserved replay buffer capacity across all surviving session contexts.
    pub replay_bytes: usize,
    /// Maximum returned output page bytes.
    pub output_page_bytes: usize,
    /// Aggregate admitted transient input bytes across all sessions.
    pub input_bytes: usize,
    /// Aggregate admitted input operations, including abandoned waits.
    pub input_slots: usize,
    /// Independent global parser, native, checkpoint and observation budgets.
    pub projection: ProjectionLimits,
}
impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            max_sessions: 512,
            max_observers: 2048,
            replay_bytes: 16 * 1024 * 1024,
            output_page_bytes: 16384,
            input_bytes: 4 * 1024 * 1024,
            input_slots: 512,
            projection: ProjectionLimits::default(),
        }
    }
}
impl RuntimeOptions {
    /// Validate global admission bounds before starting infrastructure.
    pub fn validate(&self) -> Result<(), RuntimeError> {
        if self.max_sessions == 0
            || self.max_observers == 0
            || self.output_page_bytes == 0
            || self.output_page_bytes > 65536
            || self.input_slots == 0
            || self.input_bytes == 0
        {
            return Err(RuntimeError::Capacity);
        }
        self.projection.validate()?;
        Ok(())
    }
}

/// Per-session creation choices. Raw and projected modes are fixed at creation.
#[derive(Debug, Clone)]
pub struct SessionOptions {
    /// Initial PTY dimensions.
    pub size: TerminalSize,
    /// Maximum retained replay bytes; not allocated at spawn.
    pub replay_bytes: usize,
    /// Maximum simultaneous attachments/waits for this session.
    pub max_observers: usize,
    /// OS process work and lifecycle budgets.
    pub process: ProcessLimits,
    /// None selects raw bytes; Some creates one authoritative terminal with parking.
    pub projection: Option<ProjectionOptions>,
}
impl SessionOptions {
    /// Validate local choices against the runtime's shared admission ceilings.
    /// Installed adapter capabilities are checked separately by the application.
    pub fn validate(&self, runtime: &RuntimeOptions) -> Result<(), RuntimeError> {
        self.process.validate()?;
        if self.max_observers == 0 {
            return Err(RuntimeError::Capacity);
        }
        if let Some(projection) = self.projection {
            projection.validate()?;
            if projection.terminal.size != self.size
                || self.process.read_chunk > projection.terminal.feed_bytes
                || self.process.read_chunk > projection.staging_bytes
                || self.process.read_chunk > runtime.projection.staging_bytes
                || projection.terminal.reply_bytes > runtime.projection.view_bytes
                || projection.terminal.reply_bytes > runtime.input_bytes
                || projection.terminal.reply_bytes > self.process.input_chunk
            {
                return Err(ProjectionError::InvalidConfiguration.into());
            }
        }
        Ok(())
    }

    /// Create an authoritative terminal; automatic parking is enabled by default.
    pub fn projected(terminal: TerminalConfig) -> Self {
        let mut options = Self::raw(terminal.size);
        options.projection = Some(ProjectionOptions::new(terminal));
        options
    }
    /// Construct a raw byte session with bounded default retention and work.
    pub fn raw(size: TerminalSize) -> Self {
        Self {
            size,
            replay_bytes: 65536,
            max_observers: 16,
            process: ProcessLimits::default(),
            projection: None,
        }
    }
}

/// Starting cursor for an independent attachment.
#[derive(Debug, Clone, Copy)]
pub enum AttachPosition {
    /// Start at the oldest currently retained byte.
    Oldest,
    /// Start after all output already produced.
    Tail,
    /// Resume an exact lifetime-bound cursor, reporting any gap first.
    Cursor(ReplayCursor),
}

/// Separate process, drain, and cancellation facts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SessionStatus {
    /// Actual reaped exit if known.
    pub exit: Option<ExitStatus>,
    /// Final output drain if known.
    pub drain: Option<DrainOutcome>,
    /// Supervision failed without manufacturing an exit status.
    pub supervision_error: Option<ProcessError>,
    /// Failure before any process was admitted; this is never an actual child exit.
    pub admission_error: Option<RuntimeError>,
    /// A cancellation request was admitted and remains owned by the runtime.
    pub cancellation_requested: bool,
}
impl SessionStatus {
    /// Preserve the first actual exit; contradictory reports are rejected.
    pub fn record_exit(&mut self, status: ExitStatus) -> Result<(), RuntimeError> {
        if self.exit.is_some_and(|existing| existing != status) {
            return Err(RuntimeError::Internal);
        }
        self.exit = Some(status);
        Ok(())
    }
    /// Preserve final drain independently from exit.
    pub fn record_drain(&mut self, outcome: DrainOutcome) -> Result<(), RuntimeError> {
        if self.drain.is_some_and(|existing| existing != outcome) {
            return Err(RuntimeError::Internal);
        }
        self.drain = Some(outcome);
        Ok(())
    }
    /// Record failed supervision without inventing an actual exit.
    pub fn record_failure(&mut self, error: ProcessError) {
        self.supervision_error.get_or_insert(error);
    }
    /// Fail admission without manufacturing process supervision or exit.
    pub fn record_admission_failure(&mut self, error: RuntimeError) {
        self.admission_error.get_or_insert(error);
    }
    /// Cancellation is durable intent until process supervision completes.
    pub fn admit_cancel(&mut self) -> Result<(), RuntimeError> {
        if self.completion().is_some() {
            return Err(RuntimeError::Closed);
        }
        self.cancellation_requested = true;
        Ok(())
    }

    /// Whether terminal process supervision and output draining have both finished.
    ///
    /// Supervision and draining are separate facts, so a session is finished only
    /// when *some* supervision outcome and a drain outcome are both known:
    ///
    /// ```text
    ///   completion() = Some  ⟺  (exit ∨ supervision_error ∨ admission_error) ∧ drain
    ///
    ///   exit  supervision  admission  drain │ completion
    ///   ─────────────────────────────────────────────────────────────────────
    ///    ✓        ·           ·         ✓   │ Some   normal exit, output drained
    ///    ·        ✓           ·         ✓   │ Some   supervision died; no exit invented
    ///    ·        ·           ✓         ✓   │ Some   never launched
    ///    ✓        ·           ·         ·   │ None   child gone, output still draining
    ///    ·        ·           ·         ✓   │ None   drained, child unaccounted for
    ///    ·        ·           ·         ·   │ None   still running
    /// ```
    ///
    /// A failure is never encoded as a successful exit: the three left-hand
    /// columns stay separate all the way into [`Completion`].
    pub fn completion(self) -> Option<Completion> {
        if (self.exit.is_some()
            || self.supervision_error.is_some()
            || self.admission_error.is_some())
            && self.drain.is_some()
        {
            Some(Completion { status: self })
        } else {
            None
        }
    }
}

/// Final outcome preserves real exit and incomplete drain independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Completion {
    /// Separate facts; failure is never encoded as a successful exit.
    pub status: SessionStatus,
}

/// Observation results. Completion follows retained output/gaps in cursor order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputEvent {
    /// Raw output page or exact gap. Pending is never returned as an event.
    Replay(ReplayPage),
    /// Process and drain have completed; repeated reads return this stable result.
    Complete(Completion),
}

#[cfg(test)]
mod tests;
