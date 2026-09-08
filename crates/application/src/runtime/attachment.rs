use super::{AttachPosition, OutputEvent, RuntimeError, SessionContext};
use pty_runtime_domain::{ReplayCursor, ReplayPage};
use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

/// Independent observation cursor; dropping this handle never terminates a process.
pub struct Attachment {
    pub(crate) context: Arc<SessionContext>,
    pub(crate) id: u64,
    cursor: ReplayCursor,
}
impl Attachment {
    pub(crate) fn new(
        context: Arc<SessionContext>,
        position: AttachPosition,
    ) -> Result<Self, RuntimeError> {
        let id = context.register_watcher()?;
        let result = (|| {
            let state = context.state.lock().map_err(|_| RuntimeError::Internal)?;
            let cursor = match position {
                AttachPosition::Oldest => state.replay.floor(),
                AttachPosition::Tail => state.replay.end(),
                AttachPosition::Cursor(cursor) => cursor,
            };
            state
                .replay
                .read(cursor, context.page_bytes)
                .map_err(|_| RuntimeError::InvalidCursor)?;
            Ok(cursor)
        })();
        match result {
            Ok(cursor) => Ok(Self {
                context,
                id,
                cursor,
            }),
            Err(e) => {
                context.remove_watcher(id);
                Err(e)
            }
        }
    }
    /// Next unread byte. Cursor advances only when a future returns a ready event.
    pub fn cursor(&self) -> ReplayCursor {
        self.cursor
    }
    /// Wait for one page, gap or completion without a per-observer output queue.
    /// Dropping a pending future leaves the cursor unchanged.
    pub fn read_next(&mut self) -> NextOutput<'_> {
        NextOutput { attachment: self }
    }
    /// Read immediately; None means pending rather than process completion.
    pub fn try_next(&mut self) -> Result<Option<OutputEvent>, RuntimeError> {
        let state = self
            .context
            .state
            .lock()
            .map_err(|_| RuntimeError::Internal)?;
        let result = SessionContext::read(&state, self.cursor, self.context.page_bytes)?;
        drop(state);
        if let Some(event) = &result {
            self.advance(event);
        }
        Ok(result)
    }
    fn advance(&mut self, event: &OutputEvent) {
        match event {
            OutputEvent::Replay(ReplayPage::Gap { from, to }) => {
                if let Some(diagnostics) = &self.context.diagnostics {
                    diagnostics.count(crate::diagnostics::CounterKind::ObserverGaps, 1);
                    diagnostics.count(
                        crate::diagnostics::CounterKind::ObserverGapBytes,
                        to.offset - from.offset,
                    );
                }
                self.cursor = *to;
            }
            OutputEvent::Replay(ReplayPage::Bytes { next, .. }) => self.cursor = *next,
            _ => (),
        }
    }
}
impl Drop for Attachment {
    fn drop(&mut self) {
        self.context.remove_watcher(self.id);
    }
}

/// A cancellable observation wait borrowing exactly one attachment.
pub struct NextOutput<'a> {
    attachment: &'a mut Attachment,
}
impl Future for NextOutput<'_> {
    type Output = Result<OutputEvent, RuntimeError>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let mut next_waker = Some(cx.waker().clone());
        let mut old_waker = None;
        let result = {
            let Ok(mut state) = this.attachment.context.state.lock() else {
                return Poll::Ready(Err(RuntimeError::Internal));
            };
            let result = SessionContext::read(
                &state,
                this.attachment.cursor,
                this.attachment.context.page_bytes,
            );
            if let Some(slot) = state.watchers.get_mut(&this.attachment.id) {
                old_waker = slot.take();
                if matches!(result, Ok(None)) {
                    *slot = next_waker.take();
                }
            }
            result
        };
        // Arbitrary RawWaker clone/drop callbacks never run under session state.
        drop(old_waker);
        drop(next_waker);
        match result {
            Ok(Some(event)) => {
                this.attachment.advance(&event);
                Poll::Ready(Ok(event))
            }
            Err(error) => Poll::Ready(Err(error)),
            Ok(None) => Poll::Pending,
        }
    }
}
impl Drop for NextOutput<'_> {
    fn drop(&mut self) {
        self.attachment.context.clear_waker(self.attachment.id);
    }
}
