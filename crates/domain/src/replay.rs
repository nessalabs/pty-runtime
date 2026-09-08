//! Exact bounded retention and explicit gaps, independent of observer queues.
use crate::{ReplayCursor, SessionLifetime};
use std::collections::VecDeque;

/// Cursor validation or retention arithmetic failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayError {
    /// Cursor belongs to another session or owner lifetime.
    ForeignLifetime,
    /// Cursor refers to output that has not been produced.
    FutureCursor,
    /// Absolute stream offset cannot represent another chunk.
    OffsetExhausted,
    /// A read must allow at least one byte of progress.
    EmptyPageLimit,
}

/// One bounded read result. Its Debug representation never includes payload.
#[derive(Clone, PartialEq, Eq)]
pub enum ReplayPage {
    /// Bytes no longer retained. The caller must acknowledge this gap first.
    Gap {
        /// Requested cursor, inclusive.
        from: ReplayCursor,
        /// Earliest retained cursor, exclusive end of the lost interval.
        to: ReplayCursor,
    },
    /// A bounded contiguous range of retained bytes.
    Bytes {
        /// First returned byte position.
        from: ReplayCursor,
        /// Cursor immediately after the returned bytes.
        next: ReplayCursor,
        /// Raw bytes; no UTF-8 assumption.
        bytes: Vec<u8>,
    },
    /// No new output exists at this cursor. Process completion is separate.
    Pending,
}

impl std::fmt::Debug for ReplayPage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Gap { from, to } => f
                .debug_struct("Gap")
                .field("from", from)
                .field("to", to)
                .finish(),
            Self::Bytes { from, next, bytes } => f
                .debug_struct("Bytes")
                .field("from", from)
                .field("next", next)
                .field("length", &bytes.len())
                .finish(),
            Self::Pending => f.write_str("Pending"),
        }
    }
}

/// Retained output for one lifetime. Capacity is a logical byte ceiling.
///
/// Storage grows only when output is appended. Allocator capacity and returned
/// page copies require separate application resource reservations. This object
/// neither reserves a global budget nor advances an observer cursor on read.
pub struct ReplayBuffer {
    lifetime: SessionLifetime,
    limit: usize,
    end: u64,
    bytes: VecDeque<u8>,
}

impl ReplayBuffer {
    /// Create empty retention without allocating the configured maximum.
    pub fn new(lifetime: SessionLifetime, limit: usize) -> Self {
        Self {
            lifetime,
            limit,
            end: 0,
            bytes: VecDeque::new(),
        }
    }

    /// Number of bytes logically retained.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Whether no bytes are retained (including a zero-retention policy).
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Oldest available byte position.
    pub fn floor(&self) -> ReplayCursor {
        ReplayCursor {
            lifetime: self.lifetime,
            offset: self.end - self.bytes.len() as u64,
        }
    }

    /// Position immediately after all produced output, including evicted bytes.
    pub fn end(&self) -> ReplayCursor {
        ReplayCursor {
            lifetime: self.lifetime,
            offset: self.end,
        }
    }

    /// Append produced bytes, retaining at most the configured suffix.
    /// Arithmetic failures leave this buffer unchanged.
    pub fn append(&mut self, bytes: &[u8]) -> Result<(), ReplayError> {
        let end = self
            .end
            .checked_add(bytes.len() as u64)
            .ok_or(ReplayError::OffsetExhausted)?;
        let suffix = &bytes[bytes.len().saturating_sub(self.limit)..];
        let keep_old = self.limit - suffix.len();
        let discard = self.bytes.len().saturating_sub(keep_old);
        self.bytes.drain(..discard);
        self.bytes.extend(suffix);
        self.end = end;
        Ok(())
    }

    /// Evict oldest bytes to satisfy external pressure, preserving absolute offsets.
    /// Allocator reclamation is separate from this exact logical bound.
    pub fn retain_at_most(&mut self, bytes: usize) {
        let discard = self.bytes.len().saturating_sub(bytes);
        self.bytes.drain(..discard);
    }

    /// Read without mutating cursor state. A cancelled caller may retry exactly.
    pub fn read(&self, cursor: ReplayCursor, max_bytes: usize) -> Result<ReplayPage, ReplayError> {
        if cursor.lifetime != self.lifetime {
            return Err(ReplayError::ForeignLifetime);
        }
        if cursor.offset > self.end {
            return Err(ReplayError::FutureCursor);
        }
        if max_bytes == 0 {
            return Err(ReplayError::EmptyPageLimit);
        }
        let floor = self.floor();
        if cursor.offset < floor.offset {
            return Ok(ReplayPage::Gap {
                from: cursor,
                to: floor,
            });
        }
        if cursor.offset == self.end {
            return Ok(ReplayPage::Pending);
        }
        let start = (cursor.offset - floor.offset) as usize;
        let bytes: Vec<u8> = self.bytes.range(start..).take(max_bytes).copied().collect();
        let next = ReplayCursor {
            lifetime: self.lifetime,
            offset: cursor.offset + bytes.len() as u64,
        };
        Ok(ReplayPage::Bytes {
            from: cursor,
            next,
            bytes,
        })
    }
}

#[cfg(test)]
#[path = "replay_tests.rs"]
mod tests;
