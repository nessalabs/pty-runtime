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
pub(super) struct Queues {
    pub input: VecDeque<Input>,
    pub bytes: usize,
    pub resize: Option<(TerminalSize, oneshot::Sender<Result<(), ProcessError>>)>,
}
pub(super) struct Session {
    pub pid: u32,
    pub limits: ProcessLimits,
    pub queues: Mutex<Queues>,
    pub cancel: AtomicBool,
    pub closed: AtomicBool,
    pub stop_reader: AtomicBool,
    pub reader_done: AtomicBool,
    pub reader_failed: AtomicBool,
    pub wake: Arc<UnixStream>,
    pub reader_wake: UnixStream,
}
impl Session {
    pub fn notify(&self) {
        let _ = (&*self.wake).write(&[1]);
    }
    pub fn stop(&self) {
        self.stop_reader.store(true, Ordering::Release);
        let _ = (&self.reader_wake).write(&[1]);
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
        if let Some((_, reply)) = resize {
            let _ = reply.send(Err(reason));
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
            return Err(ProcessError::Capacity);
        }
        let (tx, rx) = oneshot::channel();
        queues.bytes += bytes.len();
        queues.input.push_back(Input {
            bytes: bytes.to_vec(),
            offset: 0,
            deadline: Instant::now() + self.limits.write_timeout,
            reply: Some(tx),
            _reservation: reservation,
        });
        drop(queues);
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
        self.cancel.store(true, Ordering::Release);
        self.notify();
        Ok(())
    }
    fn resize(
        &self,
        size: TerminalSize,
    ) -> Result<ProcessOperation<Result<(), ProcessError>>, ProcessError> {
        let mut queues = self.queues.lock().map_err(|_| ProcessError::Internal)?;
        if self.closed.load(Ordering::Acquire) {
            return Err(ProcessError::Closed);
        }
        if queues.resize.is_some() {
            return Err(ProcessError::Capacity);
        }
        let (tx, rx) = oneshot::channel();
        queues.resize = Some((size, tx));
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
