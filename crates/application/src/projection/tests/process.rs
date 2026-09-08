use crate::process::{IInputReservation, IProcessSession, ProcessOperation};
use pty_runtime_domain::{
    process::{ProcessError, WriteOutcome},
    terminal::TerminalSize,
};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};
#[derive(Default)]
pub struct Process {
    pub writes: Mutex<Vec<Vec<u8>>>,
    pub reject: AtomicBool,
    pub partial: AtomicBool,
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
        self.controls.lock().unwrap().push(size);
        Ok(Box::pin(async { Ok(()) }))
    }
}
