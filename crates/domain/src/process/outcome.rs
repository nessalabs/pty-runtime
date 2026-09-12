use std::time::Duration;

/// Stable process boundary errors; native errno and diagnostic text stay outside core.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessError {
    /// Command metadata violates the portable contract.
    InvalidCommand,
    /// Canonical launch directory is outside configured roots.
    OutsideRoots,
    /// Resource or work admission is full.
    Capacity,
    /// Process or owner no longer accepts the operation.
    Closed,
    /// OS denied the requested operation.
    PermissionDenied,
    /// Required file or executable does not exist.
    NotFound,
    /// An operation timed out; this does not imply process exit.
    Timeout,
    /// Platform does not support the requested operation.
    Unsupported,
    /// An operating-system I/O operation failed.
    Io,
    /// Internal worker or synchronization failed.
    Internal,
}
impl std::fmt::Display for ProcessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ProcessError {}

/// Actual status collected from the owned child, never inferred from a timeout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitStatus {
    /// Child called exit with this code.
    Code(i32),
    /// Child terminated from a signal.
    Signal(i32),
}

/// How the reader completed, independently of actual child exit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrainOutcome {
    /// End of output reached normally.
    Eof,
    /// Drain deadline or owner shutdown deliberately cut off output.
    Truncated,
    /// Reader failed to collect all output.
    Failed(ProcessError),
}

/// Acknowledgement for one admitted input chunk; never silently resend a suffix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteOutcome {
    /// Number of bytes accepted by the OS, not necessarily consumed by the child.
    pub written: usize,
    /// Reason the complete chunk was not written, if any.
    pub error: Option<ProcessError>,
}

/// Explicit per-process budgets and lifecycle deadlines for an adapter.
#[derive(Debug, Clone)]
pub struct ProcessLimits {
    /// Maximum concurrently admitted input chunks.
    pub input_slots: usize,
    /// Maximum aggregate queued input bytes.
    pub input_bytes: usize,
    /// Maximum input bytes in one admitted chunk.
    pub input_chunk: usize,
    /// Maximum bytes returned from one OS read.
    pub read_chunk: usize,
    /// Requested dedicated reader stack size; qualify on each target.
    pub reader_stack_bytes: usize,
    /// Grace period before an admitted cancellation escalates.
    pub terminate_grace: Duration,
    /// Time to wait for inherited output handles after child exit.
    pub drain_timeout: Duration,
    /// Deadline for an admitted write on a child that stops reading.
    pub write_timeout: Duration,
}
impl Default for ProcessLimits {
    fn default() -> Self {
        Self {
            input_slots: 16,
            input_bytes: 65536,
            input_chunk: 4096,
            read_chunk: 4096,
            reader_stack_bytes: 128 * 1024,
            terminate_grace: Duration::from_millis(250),
            drain_timeout: Duration::from_millis(250),
            write_timeout: Duration::from_secs(5),
        }
    }
}
impl ProcessLimits {
    /// Deadline the guardian uses for its own escalation, strictly later than
    /// [`Self::terminate_grace`] wherever there is room for it to be.
    ///
    /// Cancellation has two independent triggers for the same kill: the owner's
    /// timer, which asks the guardian to escalate, and the guardian's own timer,
    /// which escalates whether or not the owner is still alive. The second is the
    /// reason the guardian exists — an owner that has died or wedged cannot ask
    /// for anything — so both must stay.
    ///
    /// They must not share a deadline. Firing together makes the backstop
    /// indistinguishable from the primary: no test can attribute a kill to
    /// either, so either trigger can be deleted with every test still green.
    /// Separating them gives the owner first refusal and leaves the guardian as
    /// what it is supposed to be, which is a backstop.
    ///
    /// The margin is half the grace, floored at 250 ms so a short grace still
    /// clears scheduling jitter between two processes, and capped at 5 s so a
    /// long grace does not push the backstop hours past it. At the extreme the
    /// two converge: a grace within the margin of the 24-hour ceiling clamps to
    /// the ceiling and the staging is lost, which is accepted because an owner
    /// with a day to act has not been starved of one.
    ///
    /// **What a caller sees.** [`Self::terminate_grace`] is a minimum — ADR 0004
    /// asks to "escalate once *after* the configured grace period" — so waiting
    /// longer keeps the contract and escalating earlier would break it. In the
    /// normal case the owner escalates at exactly `terminate_grace` and this
    /// value is never reached. It is reached only when the owner relayed the
    /// terminate and then stopped asking, and that is the case where the kill
    /// lands here instead: up to 5 s later than the configured grace, never more.
    pub fn guardian_grace(&self) -> Duration {
        const CEILING: Duration = Duration::from_secs(86400);
        const FLOOR: Duration = Duration::from_millis(250);
        const CAP: Duration = Duration::from_secs(5);
        let margin = (self.terminate_grace / 2).clamp(FLOOR, CAP);
        self.terminate_grace.saturating_add(margin).min(CEILING)
    }

    /// Reject zero/inconsistent budgets before starting workers or children.
    pub fn validate(&self) -> Result<(), ProcessError> {
        if self.input_slots == 0
            || self.input_chunk == 0
            || self.input_bytes < self.input_chunk
            || self.read_chunk == 0
            || self.read_chunk > 65536
            || self.reader_stack_bytes < 65536
            || self.write_timeout.is_zero()
            || self.drain_timeout.is_zero()
            || self.terminate_grace > Duration::from_secs(86400)
            || self.drain_timeout > Duration::from_secs(86400)
            || self.write_timeout > Duration::from_secs(86400)
            || self.reader_stack_bytes > 8 * 1024 * 1024
        {
            return Err(ProcessError::Capacity);
        }
        Ok(())
    }
}
