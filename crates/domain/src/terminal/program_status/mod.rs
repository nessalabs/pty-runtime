//! Transactional OSC 7501 revision 0.4 scanning and session-owned records.
//! No output is stripped and no replies are written by this domain module.
mod report;
mod scanner;
mod store;
mod value;
pub use store::*;
pub use value::*;

/// The fixed, minimal feature-detection reply (always the ESC-backslash terminator).
pub const PROGRAM_STATUS_REPLY: &[u8] = b"\x1b]7501;?\x1b\\";
/// Hard bound on a single transactional scan. Larger feeds must be divided by the caller.
pub const MAX_SCAN_BYTES: usize = 1024 * 1024;
/// Stable failures that never include program output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramStatusError {
    /// The requested record cap is outside 64..=256.
    InvalidRecordLimit,
    /// The chunk exceeds MAX_SCAN_BYTES.
    ChunkTooLarge,
    /// A plan belongs to another store or an earlier committed generation.
    StalePlan,
    /// Monotonic counters cannot safely admit another operation.
    RevisionExhausted,
}

#[cfg(test)]
mod tests;
