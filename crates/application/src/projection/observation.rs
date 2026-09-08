use super::{ProjectionError, budgets::Lease};
use pty_runtime_domain::{
    ReplayCursor,
    terminal::{RestorationProgress, TerminalCheckpoint, TerminalView},
};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Waker},
};
/// An admitted bounded wait. Abandonment cancels observation only, never an admitted resize.
pub type ProjectionOperation<T> =
    Pin<Box<dyn Future<Output = Result<T, ProjectionError>> + Send + 'static>>;
/// Immutable copied view whose global memory reservation follows consumer ownership.
/// Borrowing cannot detach the buffer from its lease; consumers may make their own copies.
pub struct ProjectedView {
    pub(super) view: TerminalView,
    pub(super) processed: ReplayCursor,
    pub(super) control_generation: u64,
    pub(super) history: RestorationProgress,
    pub(super) _lease: Lease,
}
impl ProjectedView {
    /// Exact processed byte boundary copied with this native observation.
    pub fn processed(&self) -> ReplayCursor {
        self.processed
    }
    /// Exact successful control generation copied with this native observation.
    pub fn control_generation(&self) -> u64 {
        self.control_generation
    }
    /// History completeness at extraction, independent from later restoration work.
    pub fn restoration_progress(&self) -> RestorationProgress {
        self.history
    }
    /// Borrow cells, cursor, modes and palette independent of native ownership.
    pub fn view(&self) -> &TerminalView {
        &self.view
    }
}
/// Immutable binary state with exact byte/control identity and a retained memory reservation.
/// This plaintext transfer pin is distinct from the encrypted provider representation.
pub struct PinnedCheckpoint {
    pub(super) checkpoint: TerminalCheckpoint,
    pub(super) _lease: Lease,
}
impl PinnedCheckpoint {
    /// Borrow compatible binary state and exact processed/control boundary.
    pub fn checkpoint(&self) -> &TerminalCheckpoint {
        &self.checkpoint
    }
}
struct ResultState<T> {
    result: Option<Result<T, ProjectionError>>,
    waker: Option<Waker>,
    complete: bool,
}
pub(super) struct Ticket<T> {
    state: Mutex<ResultState<T>>,
    cancelled: AtomicBool,
    _lease: Option<Lease>,
}
impl<T: Send + 'static> Ticket<T> {
    pub fn new(lease: Option<Lease>) -> (Arc<Self>, ProjectionOperation<T>) {
        let ticket = Arc::new(Self {
            state: Mutex::new(ResultState {
                result: None,
                waker: None,
                complete: false,
            }),
            cancelled: AtomicBool::new(false),
            _lease: lease,
        });
        (ticket.clone(), Box::pin(Wait(ticket)))
    }
    pub fn complete(&self, result: Result<T, ProjectionError>) {
        let wake = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if state.complete {
                return;
            }
            state.complete = true;
            if !self.cancelled.load(Ordering::Acquire) {
                state.result = Some(result);
            }
            state.waker.take()
        };
        if let Some(wake) = wake {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| wake.wake()));
        }
    }
    pub fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}
struct Wait<T>(Arc<Ticket<T>>);
impl<T: Send + 'static> Future for Wait<T> {
    type Output = Result<T, ProjectionError>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut next_waker = Some(cx.waker().clone());
        let (result, old_waker) = {
            let mut state = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
            let old_waker = state.waker.take();
            let result = if let Some(result) = state.result.take() {
                Poll::Ready(result)
            } else if state.complete {
                Poll::Ready(Err(ProjectionError::Closed))
            } else {
                state.waker = next_waker.take();
                Poll::Pending
            };
            (result, old_waker)
        };
        drop(old_waker);
        drop(next_waker);
        result
    }
}
impl<T> Drop for Wait<T> {
    fn drop(&mut self) {
        self.0.cancelled.store(true, Ordering::Release);
        let mut state = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
        let waker = state.waker.take();
        // Result destructors can release arbitrary caller/provider-owned data.
        let result = state.result.take();
        drop(state);
        drop(waker);
        drop(result);
    }
}
