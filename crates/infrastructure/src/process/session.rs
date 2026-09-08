use super::error;
use futures_channel::oneshot;
use pty_runtime_application::process::{IInputReservation, IProcessSession, ProcessOperation};
use pty_runtime_domain::{
    process::{ProcessError, ProcessLimits, WriteOutcome},
    terminal::TerminalSize,
};
use std::{
    collections::VecDeque,
    io::Write,
    os::unix::net::UnixStream,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};
pub(super) struct Input {
    pub timing: Option<pty_runtime_application::diagnostics::Timing>,
    pub bytes: Vec<u8>,
    pub offset: usize,
    pub deadline: Instant,
    pub reply: Option<oneshot::Sender<WriteOutcome>>,
    pub _reservation: Option<Box<dyn IInputReservation>>,
}
impl Drop for Input {
    fn drop(&mut self) {
        // Volatile stores prevent optimization from eliding transient input clearing.
        for byte in &mut self.bytes {
            // SAFETY: each pointer refers to an exclusively borrowed initialized byte.
            unsafe {
                std::ptr::write_volatile(byte, 0);
            }
        }
    }
}
pub(super) struct Resize {
    pub size: TerminalSize,
    pub reply: oneshot::Sender<Result<(), ProcessError>>,
    pub timing: Option<pty_runtime_application::diagnostics::Timing>,
}
pub(super) struct Queues {
    pub input: VecDeque<Input>,
    pub bytes: usize,
    pub resize: Option<Resize>,
}
pub(super) struct Session {
    pub diagnostics: Option<Arc<pty_runtime_application::diagnostics::RuntimeDiagnostics>>,
    pub pid: u32,
    pub limits: ProcessLimits,
    pub queues: Mutex<Queues>,
    pub cancel: AtomicBool,
    pub cancel_timing: Mutex<Option<pty_runtime_application::diagnostics::Timing>>,
    pub closed: AtomicBool,
    pub stop_reader: AtomicBool,
    pub reader_done: AtomicBool,
    pub reader_failed: AtomicBool,
    pub wake: Mutex<Option<Arc<UnixStream>>>,
    pub reader_wake: Mutex<Option<UnixStream>>,
}
impl Session {
    pub fn notify(&self) {
        if let Ok(wake) = self.wake.lock() {
            if let Some(wake) = wake.as_ref() {
                let _ = (&**wake).write(&[1]);
            }
        }
    }
    pub fn stop(&self) {
        self.stop_reader.store(true, Ordering::Release);
        if let Ok(wake) = self.reader_wake.lock() {
            if let Some(wake) = wake.as_ref() {
                let _ = (&*wake).write(&[1]);
            }
        }
    }
    pub fn release_wakes(&self) {
        if let Ok(mut wake) = self.wake.lock() {
            wake.take();
        }
        if let Ok(mut wake) = self.reader_wake.lock() {
            wake.take();
        }
    }
    pub fn finish_inputs(&self, reason: ProcessError) {
        self.closed.store(true, Ordering::Release);
        let Ok(mut queues) = self.queues.lock() else {
            return;
        };
        let input = std::mem::take(&mut queues.input);
        let resize = queues.resize.take();
        queues.bytes = 0;
        drop(queues);
        if let Some(diagnostics) = &self.diagnostics {
            diagnostics.count(
                pty_runtime_application::diagnostics::CounterKind::FailedOperations,
                input.len() as u64 + u64::from(resize.is_some()),
            );
        }
        for mut input in input {
            if let Some(reply) = input.reply.take() {
                let written = input.offset;
                drop(input);
                let _ = reply.send(WriteOutcome {
                    written,
                    error: Some(reason),
                });
            }
        }
        if let Some(resize) = resize {
            let _ = resize.reply.send(Err(reason));
        }
    }
}
impl IProcessSession for Session {
    fn process_id(&self) -> u32 {
        self.pid
    }
    fn write_reserved(
        &self,
        bytes: &[u8],
        reservation: Option<Box<dyn IInputReservation>>,
    ) -> Result<ProcessOperation<WriteOutcome>, ProcessError> {
        let mut queues = self.queues.lock().map_err(|_| ProcessError::Internal)?;
        if self.closed.load(Ordering::Acquire) {
            return Err(ProcessError::Closed);
        }
        if bytes.len() > self.limits.input_chunk
            || queues.input.len() >= self.limits.input_slots
            || bytes.len() > self.limits.input_bytes.saturating_sub(queues.bytes)
        {
            if let Some(diagnostics) = &self.diagnostics {
                diagnostics.count(
                    pty_runtime_application::diagnostics::CounterKind::InputSaturation,
                    1,
                );
            }
            return Err(ProcessError::Capacity);
        }
        let (tx, rx) = oneshot::channel();
        queues.bytes += bytes.len();
        let timing = self.diagnostics.as_ref().map(|diagnostics| {
            pty_runtime_application::diagnostics::Timing::new(
                diagnostics.clone(),
                pty_runtime_application::diagnostics::LatencyKind::InputDispatch,
                Instant::now(),
            )
        });
        queues.input.push_back(Input {
            timing,
            bytes: bytes.to_vec(),
            offset: 0,
            deadline: Instant::now() + self.limits.write_timeout,
            reply: Some(tx),
            _reservation: reservation,
        });
        drop(queues);
        if let Some(diagnostics) = &self.diagnostics {
            diagnostics.count(
                pty_runtime_application::diagnostics::CounterKind::InputAdmittedBytes,
                bytes.len() as u64,
            );
        }
        self.notify();
        Ok(Box::pin(async move {
            rx.await.unwrap_or(WriteOutcome {
                written: 0,
                error: Some(ProcessError::Internal),
            })
        }))
    }
    fn request_cancel(&self) -> Result<(), ProcessError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(ProcessError::Closed);
        }
        let mut pending = self
            .cancel_timing
            .lock()
            .map_err(|_| ProcessError::Internal)?;
        // Closing is published before the owner drains this same timer slot.
        // Recheck under its lock so a retained closed handle cannot install a
        // measurement after final cleanup has already observed an empty slot.
        if self.closed.load(Ordering::Acquire) {
            return Err(ProcessError::Closed);
        }
        if !self.cancel.load(Ordering::Acquire) {
            *pending = self.diagnostics.as_ref().map(|diagnostics| {
                pty_runtime_application::diagnostics::Timing::new(
                    diagnostics.clone(),
                    pty_runtime_application::diagnostics::LatencyKind::CancelDispatch,
                    Instant::now(),
                )
            });
            self.cancel.store(true, Ordering::Release);
        }
        drop(pending);
        self.notify();
        Ok(())
    }
    fn resize(
        &self,
        size: TerminalSize,
    ) -> Result<ProcessOperation<Result<(), ProcessError>>, ProcessError> {
        let timing = self.diagnostics.as_ref().map(|diagnostics| {
            pty_runtime_application::diagnostics::Timing::new(
                diagnostics.clone(),
                pty_runtime_application::diagnostics::LatencyKind::ResizeDispatch,
                Instant::now(),
            )
        });
        self.resize_timed(size, timing)
    }
    fn resize_timed(
        &self,
        size: TerminalSize,
        timing: Option<pty_runtime_application::diagnostics::Timing>,
    ) -> Result<ProcessOperation<Result<(), ProcessError>>, ProcessError> {
        let mut queues = self.queues.lock().map_err(|_| ProcessError::Internal)?;
        if self.closed.load(Ordering::Acquire) {
            return Err(ProcessError::Closed);
        }
        if queues.resize.is_some() {
            return Err(ProcessError::Capacity);
        }
        let (tx, rx) = oneshot::channel();
        queues.resize = Some(Resize {
            size,
            reply: tx,
            timing,
        });
        drop(queues);
        self.notify();
        Ok(Box::pin(async move {
            rx.await.unwrap_or(Err(ProcessError::Internal))
        }))
    }
}
pub(super) fn pair() -> Result<(UnixStream, UnixStream), ProcessError> {
    let (a, b) = UnixStream::pair().map_err(error)?;
    a.set_nonblocking(true).map_err(error)?;
    b.set_nonblocking(true).map_err(error)?;
    Ok((a, b))
}
