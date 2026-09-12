use crate::process::{IInputReservation, IProcessSession, ProcessOperation};
use pty_runtime_domain::{
    process::{ProcessError, WriteOutcome},
    terminal::TerminalSize,
};
use std::{
    future::poll_fn,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    task::Poll,
};
#[derive(Default)]
pub struct Process {
    pub writes: Mutex<Vec<Vec<u8>>>,
    pub reject: AtomicBool,
    /// Refuse a write with a non-`Capacity` error, which fails the projection
    /// rather than asking it to retry.
    pub reject_io: AtomicBool,
    pub partial: AtomicBool,
    pub fail_resize_admission: AtomicBool,
    pub fail_resize_completion: AtomicBool,
    /// Keep an admitted resize pending until this is cleared, so a test can hold
    /// native control work in flight across worker runs.
    pub hold_resize: Arc<AtomicBool>,
    pub controls: Mutex<Vec<TerminalSize>>,
}
impl IProcessSession for Process {
    fn process_id(&self) -> u32 {
        42
    }
    fn write_reserved(
        &self,
        bytes: &[u8],
        reservation: Option<Box<dyn IInputReservation>>,
    ) -> Result<ProcessOperation<WriteOutcome>, ProcessError> {
        if self.reject.load(Ordering::Acquire) {
            return Err(ProcessError::Capacity);
        }
        if self.reject_io.load(Ordering::Acquire) {
            return Err(ProcessError::Io);
        }
        self.writes.lock().unwrap().push(bytes.to_vec());
        let count = bytes.len();
        drop(reservation);
        let partial = self.partial.load(Ordering::Acquire);
        Ok(Box::pin(async move {
            WriteOutcome {
                written: if partial { 0 } else { count },
                error: partial.then_some(ProcessError::Io),
            }
        }))
    }
    fn request_cancel(&self) -> Result<(), ProcessError> {
        Ok(())
    }
    fn resize(
        &self,
        size: TerminalSize,
    ) -> Result<ProcessOperation<Result<(), ProcessError>>, ProcessError> {
        if self.fail_resize_admission.load(Ordering::Acquire) {
            return Err(ProcessError::Io);
        }
        self.controls.lock().unwrap().push(size);
        let fail = self.fail_resize_completion.load(Ordering::Acquire);
        let hold = self.hold_resize.clone();
        Ok(Box::pin(poll_fn(move |_| {
            if hold.load(Ordering::Acquire) {
                return Poll::Pending;
            }
            Poll::Ready(if fail { Err(ProcessError::Io) } else { Ok(()) })
        })))
    }
}
