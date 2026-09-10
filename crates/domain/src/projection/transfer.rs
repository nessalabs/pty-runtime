use super::ProjectionError;
use crate::{ReplayCursor, SessionLifetime, process::DrainOutcome};

/// Independent next-event cursor. Resizes advance sequence without consuming bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferCursor {
    /// Runtime-issued lifetime; cursors never cross session lifetimes.
    pub lifetime: SessionLifetime,
    /// Number of successful model mutations before the next requested event.
    pub sequence: u64,
}
/// Exact state represented before the next transfer event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferBoundary {
    /// Ordered original-output/control continuation position.
    pub cursor: TransferCursor,
    /// Original PTY bytes successfully fed to the authoritative model.
    pub processed: ReplayCursor,
    /// Successful ordered OS/model resize generation.
    pub control_generation: u64,
}
/// Explicit continuation rejection; no gap is interpreted as an empty event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferError {
    /// Cursor belongs to another lifetime.
    ForeignLifetime,
    /// Cursor is ahead of all successfully applied events.
    FutureCursor,
    /// Required events were evicted or could not be retained; obtain a new snapshot.
    ResyncRequired {
        /// Earliest currently retained next-event cursor.
        oldest: TransferCursor,
    },
    /// Runtime ownership ended; retained observers cannot keep it alive.
    Closed,
    /// Ordered sequence is exhausted or its invariant failed; obtain another lifetime.
    Unavailable,
}
/// Immutable end of the applied prefix, distinct from process completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferEnd {
    /// Last completely applied event boundary.
    pub boundary: TransferBoundary,
    /// Drain fact known when this end was recorded; None on an early parser failure.
    pub drain: Option<DrainOutcome>,
    /// None means all admitted output/controls reached the final drained boundary.
    /// Some terminates only the valid prefix; it never claims parser catchup.
    pub projection_failure: Option<ProjectionError>,
}
/// Pure ordering/retention policy; application owns payloads and quota leases.
///
/// `sequence` counts successful model mutations, not bytes. A resize advances it
/// without moving `processed`, which is what lets one cursor address both kinds
/// of continuation event:
///
/// ```text
///   start                    seq 0   processed 0    ctrl 0
///   output(processed = 10)   seq 1   processed 10   ctrl 0
///   resized(generation = 1)  seq 2   processed 10   ctrl 1   ← seq moves, bytes do not
///   output(processed = 15)   seq 3   processed 15   ctrl 1
///   seal(drain = Eof)        end = TransferEnd { boundary: seq 3, .. }
/// ```
///
/// A cursor is only readable while it sits in `oldest ..= boundary.cursor`.
/// Eviction (`discard_before`) raises `oldest`, so an observer that falls behind
/// gets `ResyncRequired { oldest }` rather than a silently shortened stream.
pub struct TransferOrder {
    boundary: TransferBoundary,
    oldest: u64,
    end: Option<TransferEnd>,
    closed: bool,
    unavailable: bool,
}
impl TransferOrder {
    /// Begin at an empty model. Sequence advances even without observers.
    pub fn new(lifetime: SessionLifetime) -> Self {
        Self {
            boundary: TransferBoundary {
                cursor: TransferCursor {
                    lifetime,
                    sequence: 0,
                },
                processed: ReplayCursor {
                    lifetime,
                    offset: 0,
                },
                control_generation: 0,
            },
            oldest: 0,
            end: None,
            closed: false,
            unavailable: false,
        }
    }
    /// Current checkpoint-compatible continuation boundary.
    pub fn boundary(&self) -> TransferBoundary {
        self.boundary
    }
    /// Advance exactly once after one successful native feed.
    pub fn output(&mut self, processed: ReplayCursor) -> Result<TransferBoundary, TransferError> {
        self.require_append()?;
        if processed.lifetime != self.boundary.processed.lifetime
            || processed.offset <= self.boundary.processed.offset
        {
            return Err(TransferError::Unavailable);
        }
        self.advance()?;
        self.boundary.processed = processed;
        Ok(self.boundary)
    }
    /// Advance exactly once after a successful ordered OS/model resize.
    pub fn resized(&mut self, generation: u64) -> Result<TransferBoundary, TransferError> {
        self.require_append()?;
        if self.boundary.control_generation.checked_add(1) != Some(generation) {
            return Err(TransferError::Unavailable);
        }
        self.advance()?;
        self.boundary.control_generation = generation;
        Ok(self.boundary)
    }
    fn advance(&mut self) -> Result<(), TransferError> {
        self.boundary.cursor.sequence = self
            .boundary
            .cursor
            .sequence
            .checked_add(1)
            .ok_or(TransferError::Unavailable)?;
        Ok(())
    }
    /// Discard a prefix through a known boundary; holes invalidate all earlier cursors.
    pub fn discard_before(&mut self, cursor: TransferCursor) -> Result<(), TransferError> {
        self.validate_identity(cursor)?;
        self.oldest = self.oldest.max(cursor.sequence);
        Ok(())
    }
    /// Validate identity, future position, and retained range in that order.
    pub fn validate(&self, cursor: TransferCursor) -> Result<(), TransferError> {
        self.require_readable()?;
        self.validate_identity(cursor)?;
        if cursor.sequence < self.oldest {
            return Err(TransferError::ResyncRequired {
                oldest: TransferCursor {
                    sequence: self.oldest,
                    ..self.boundary.cursor
                },
            });
        }
        Ok(())
    }
    /// Observe the immutable terminal prefix, if sealed. Reading it never upgrades
    /// an earlier failure to a later successful drain fact.
    pub fn end(&self) -> Option<TransferEnd> {
        self.end
    }
    /// Capture a valid terminal prefix once; successful endings require a drain fact.
    pub fn seal(
        &mut self,
        drain: Option<DrainOutcome>,
        failure: Option<ProjectionError>,
    ) -> Result<(), TransferError> {
        self.require_readable()?;
        if self.end.is_none() {
            if drain.is_none() && failure.is_none() {
                return Err(TransferError::Unavailable);
            }
            self.end = Some(TransferEnd {
                boundary: self.boundary,
                drain,
                projection_failure: failure,
            });
        }
        Ok(())
    }
    /// Whether another successful mutation can extend this stream.
    pub fn accepts_events(&self) -> bool {
        !self.closed && !self.unavailable && self.end.is_none()
    }
    /// Return a checkpoint boundary only while observer access remains available.
    pub fn open_boundary(&self) -> Result<TransferBoundary, TransferError> {
        self.require_readable()?;
        Ok(self.boundary)
    }
    /// End runtime ownership permanently; old observer cursors become closed.
    pub fn close(&mut self) {
        self.closed = true;
    }
    /// Sequence/invariant failure permanently prevents continuation or new observers.
    pub fn invalidate(&mut self) {
        self.unavailable = true;
    }
    fn require_readable(&self) -> Result<(), TransferError> {
        if self.closed {
            return Err(TransferError::Closed);
        }
        if self.unavailable {
            return Err(TransferError::Unavailable);
        }
        Ok(())
    }
    fn require_append(&self) -> Result<(), TransferError> {
        self.require_readable()?;
        if self.end.is_some() {
            return Err(TransferError::Closed);
        }
        Ok(())
    }
    fn validate_identity(&self, cursor: TransferCursor) -> Result<(), TransferError> {
        if cursor.lifetime != self.boundary.cursor.lifetime {
            return Err(TransferError::ForeignLifetime);
        }
        if cursor.sequence > self.boundary.cursor.sequence {
            return Err(TransferError::FutureCursor);
        }
        Ok(())
    }
}
