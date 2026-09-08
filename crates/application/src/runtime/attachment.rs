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
            OutputEvent::Replay(ReplayPage::Gap { to, .. }) => self.cursor = *to,
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
        let result = {
            let Ok(mut state) = this.attachment.context.state.lock() else {
                return Poll::Ready(Err(RuntimeError::Internal));
            };
            match SessionContext::read(
                &state,
                this.attachment.cursor,
                this.attachment.context.page_bytes,
            ) {
                Ok(None) => {
                    if let Some(slot) = state.watchers.get_mut(&this.attachment.id) {
                        *slot = Some(cx.waker().clone());
                    }
                    return Poll::Pending;
                }
                other => other,
            }
        };
        match result {
            Ok(Some(event)) => {
                this.attachment.advance(&event);
                Poll::Ready(Ok(event))
            }
            Err(e) => Poll::Ready(Err(e)),
            Ok(None) => Poll::Pending,
        }
    }
}
impl Drop for NextOutput<'_> {
    fn drop(&mut self) {
        self.attachment.context.clear_waker(self.attachment.id);
    }
}
