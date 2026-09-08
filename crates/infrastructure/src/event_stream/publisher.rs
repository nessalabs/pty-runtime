use super::{PublicationError, codec};
use ::event_stream::{CURSOR_VERSION, Cursor, EventId, EventSink, NewEvent, StreamKey};
use pty_runtime_application::runtime::{Attachment, OutputEvent};
use pty_runtime_domain::{ReplayCursor, session::RuntimeError};
use std::sync::Arc;
struct Source {
    from: ReplayCursor,
    event: OutputEvent,
}
struct Prepared {
    event: NewEvent,
    after: ReplayCursor,
    complete: bool,
}
enum Pending {
    Source(Source),
    Prepared(Prepared),
}

/// One validated publication acknowledgement, with two deliberately distinct cursors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Publication {
    /// Cursor assigned by the external event store; other writers may interleave.
    pub event_cursor: Cursor,
    /// PTY byte position acknowledged by this publisher.
    pub byte_cursor: ReplayCursor,
    /// Whether this receipt acknowledges the terminal completion event.
    pub complete: bool,
}
/// Caller-driven bridge from an independent raw attachment to a caller-owned sink.
///
/// At most one page is pending. A page is capped at 65,536 bytes by runtime policy;
/// encoding temporarily retains the page plus two encoded copies (each at most
/// 65,600 bytes), then retains only one immutable payload. The attachment consumes
/// an existing global/per-session observer slot. Sink-owned queues and retention
/// have their own external budgets. No sink await holds a PTY/model/runtime lock.
///
/// Dropping a publication wait preserves the exact pending ID/schema/payload.
/// Call `publish_next` again to retry it; every error leaves pending state intact.
/// Dropping the publisher detaches its observer and forfeits its pending retry
/// state, but does not cancel already accepted sink work or terminate the PTY.
/// A newly created publisher has a new identity and does not deduplicate an old
/// uncertain append. Use event-store cursors for subscription reconnects.
pub struct EventStreamPublisher {
    attachment: Attachment,
    sink: Arc<dyn EventSink>,
    stream: StreamKey,
    identity: u64,
    sequence: u64,
    acknowledged: ReplayCursor,
    last_event_offset: u64,
    pending: Option<Pending>,
    complete: bool,
}
impl EventStreamPublisher {
    /// Consume one quota-counted raw observer and an explicitly resolved stream
    /// incarnation. Does not create a stream, task, worker or persistence facility.
    pub fn new(
        attachment: Attachment,
        sink: Arc<dyn EventSink>,
        stream: StreamKey,
    ) -> Result<Self, PublicationError> {
        let identity = crate::identity::next_owner_identity()
            .map_err(|e| PublicationError::Source(RuntimeError::Process(e)))?;
        let acknowledged = attachment.cursor();
        Ok(Self {
            attachment,
            sink,
            stream,
            identity,
            sequence: 0,
            acknowledged,
            last_event_offset: 0,
            pending: None,
            complete: false,
        })
    }
    /// Last PTY byte position acknowledged by a validated external receipt.
    /// A failed/cancelled append never advances it even though raw observation did.
    pub fn acknowledged_byte_cursor(&self) -> ReplayCursor {
        self.acknowledged
    }
    /// Identity to reconcile an uncertain append. None also covers a retained source
    /// page whose encoding failed before any external publication attempt.
    pub fn pending_event_id(&self) -> Option<&EventId> {
        match &self.pending {
            Some(Pending::Prepared(pending)) => Some(&pending.event.id),
            _ => None,
        }
    }
    /// Wait for and publish one source event, or retry the exact pending event.
    /// Returns None only after completion was acknowledged. No automatic retries
    /// occur; callers choose retry timing, cancellation and external failure policy.
    pub async fn publish_next(&mut self) -> Result<Option<Publication>, PublicationError> {
        if self.complete {
            return Ok(None);
        }
        if self.pending.is_none() {
            self.sequence
                .checked_add(1)
                .ok_or(PublicationError::Capacity)?;
            let from = self.attachment.cursor();
            let event = self.attachment.read_next().await?;
            // Install source ownership before any fallible conversion; the source
            // observer has already advanced and must not be read again on failure.
            self.pending = Some(Pending::Source(Source { from, event }));
        }
        self.prepare()?;
        let Some(Pending::Prepared(pending)) = &self.pending else {
            return Err(PublicationError::InvalidRecord);
        };
        let receipt = self
            .sink
            .append(&self.stream, pending.event.clone())
            .await?;
        if receipt.record.event != pending.event
            || receipt.record.cursor.stream != self.stream
            || receipt.record.cursor.version != CURSOR_VERSION
            || receipt.record.cursor.offset <= self.last_event_offset
        {
            return Err(PublicationError::InvalidRecord);
        }
        let result = Publication {
            event_cursor: receipt.record.cursor.clone(),
            byte_cursor: pending.after,
            complete: pending.complete,
        };
        self.last_event_offset = receipt.record.cursor.offset;
        self.acknowledged = pending.after;
        self.complete = pending.complete;
        self.pending = None;
        Ok(Some(result))
    }
    fn prepare(&mut self) -> Result<(), PublicationError> {
        let Some(Pending::Source(source)) = &self.pending else {
            return Ok(());
        };
        let sequence = self
            .sequence
            .checked_add(1)
            .ok_or(PublicationError::Capacity)?;
        let (payload, after, complete) = codec::encode(source.from, &source.event)?;
        let event = NewEvent {
            id: EventId::new(format!(
                "pty-{:016x}-{:016x}-{:016x}-{:016x}",
                source.from.lifetime.owner(),
                source.from.lifetime.sequence(),
                self.identity,
                sequence
            ))
            .map_err(|_| PublicationError::Capacity)?,
            schema: codec::schema()?,
            payload,
        };
        self.pending = Some(Pending::Prepared(Prepared {
            event,
            after,
            complete,
        }));
        self.sequence = sequence;
        Ok(())
    }
}
