use super::{
    PinnedCheckpoint,
    budgets::Lease,
    journal::{Journal, Record},
};
use pty_runtime_domain::{
    projection::{TransferBoundary, TransferCursor, TransferEnd, TransferError},
    terminal::{ControlGeneration, TerminalSize},
};
use std::{
    sync::Arc,
    task::{Context, Poll},
};

/// A checkpoint and continuation opened at the same applied event boundary.
/// Restore its checkpoint in a compatible consumer, then apply original bytes and
/// resize events in order. Discard consumer-generated replies; only the server replies.
/// The snapshot pin and observer have independent lifetimes after `into_parts`.
pub struct StateTransfer {
    pub(super) checkpoint: PinnedCheckpoint,
    pub(super) observer: TransferObserver,
}
impl StateTransfer {
    /// Borrow compatible state; memory stays reserved while this pin survives.
    pub fn checkpoint(&self) -> &PinnedCheckpoint {
        &self.checkpoint
    }
    /// Exact byte/control/event boundary represented by the checkpoint.
    pub fn boundary(&self) -> TransferBoundary {
        self.observer.start
    }
    /// Separate checkpoint memory from the continuation subscription.
    pub fn into_parts(self) -> (PinnedCheckpoint, TransferObserver) {
        (self.checkpoint, self.observer)
    }
}
/// One bounded observer registration; no private output queue or runtime ownership.
/// Dropping it deregisters its waker. Dropping the final observer releases journal
/// retention; events already returned retain their own reservations until dropped.
pub struct TransferObserver {
    pub(super) journal: Arc<Journal>,
    pub(super) id: u64,
    pub(super) start: TransferBoundary,
    pub(super) _permit: Lease,
}
impl TransferObserver {
    /// Initial continuation boundary paired with this observer's checkpoint.
    pub fn boundary(&self) -> TransferBoundary {
        self.start
    }
    /// Read at an explicit independent cursor without blocking or advancing it.
    /// Retention loss returns ResyncRequired; request a fresh transfer to recover.
    pub fn read(&self, cursor: TransferCursor) -> Result<TransferRead, TransferError> {
        self.journal.read(self.id, self.start.cursor, cursor, None)
    }
    /// Register one replaceable waker only while this exact cursor is pending.
    /// Re-poll after waking; callers advance using TransferEvent::after().cursor. This
    /// method borrows the observer, so cancellation can clear its registration.
    pub fn poll_read(
        &mut self,
        cursor: TransferCursor,
        cx: &mut Context<'_>,
    ) -> Poll<Result<TransferRead, TransferError>> {
        match self
            .journal
            .read(self.id, self.start.cursor, cursor, Some(cx.waker()))
        {
            Ok(TransferRead::Pending) => Poll::Pending,
            result => Poll::Ready(result),
        }
    }
    /// Cancel a pending notification without dropping the observer or changing its cursor.
    pub fn cancel_wait(&mut self) {
        self.journal.cancel_wait(self.id);
    }
}
impl Drop for TransferObserver {
    fn drop(&mut self) {
        self.journal.remove(self.id);
    }
}
/// One continuation read; Pending is never evidence of parser completion.
pub enum TransferRead {
    /// Successful model mutation; payload ownership remains quota-charged.
    Event(TransferEvent),
    /// No event yet; projection may lag process drain.
    Pending,
    /// Immutable successful final boundary or explicitly failed applied prefix.
    End(TransferEnd),
}
/// Shared immutable event; cloning shares payload and its reservation.
#[derive(Clone)]
pub struct TransferEvent(pub(super) Arc<Record>);
impl TransferEvent {
    /// State boundary immediately after applying this event.
    pub fn after(&self) -> TransferBoundary {
        self.0.after
    }
    /// Original bytes or applied resize. Consumers must suppress generated replies.
    pub fn kind(&self) -> TransferEventKind<'_> {
        match &self.0.kind {
            super::journal::RecordKind::Output(bytes) => TransferEventKind::Output(bytes),
            super::journal::RecordKind::Resize { size, generation } => TransferEventKind::Resize {
                size: *size,
                generation: *generation,
            },
        }
    }
}
/// Borrowed successful authoritative mutation, with no formatted-output substitution.
pub enum TransferEventKind<'a> {
    /// Exact original PTY bytes fed once to the server model.
    Output(&'a [u8]),
    /// Dimensions accepted by both the OS and authoritative model.
    Resize {
        /// Applied dimensions.
        size: TerminalSize,
        /// Applied ordered control generation.
        generation: ControlGeneration,
    },
}

/// A single observer wait. Dropping it clears the registered waker without
/// advancing the caller's cursor or cancelling authoritative model work.
pub struct TransferWait<'a> {
    observer: &'a mut TransferObserver,
    cursor: TransferCursor,
}
impl TransferObserver {
    /// Wait for this cursor to become readable. Only one wait can borrow this
    /// observer at once; other observers continue independently.
    pub fn wait(&mut self, cursor: TransferCursor) -> TransferWait<'_> {
        TransferWait {
            observer: self,
            cursor,
        }
    }
}
impl std::future::Future for TransferWait<'_> {
    type Output = Result<TransferRead, TransferError>;
    fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let cursor = self.cursor;
        self.observer.poll_read(cursor, cx)
    }
}
impl Drop for TransferWait<'_> {
    fn drop(&mut self) {
        self.observer.cancel_wait();
    }
}
