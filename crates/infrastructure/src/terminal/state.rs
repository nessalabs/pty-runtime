use super::{COMPATIBILITY, GhosttyTerminal, buffer, ffi};
use pty_runtime_application::terminal::ITerminal;
use pty_runtime_domain::terminal::*;

impl ITerminal for GhosttyTerminal {
    fn feed(&mut self, bytes: &[u8]) -> Result<TerminalEffects, TerminalError> {
        self.healthy()?;
        if bytes.len() > self.config.feed_bytes {
            return Err(TerminalError::BudgetExceeded);
        }
        if self.restoring.is_some() {
            return Err(TerminalError::HistoryIncomplete);
        }
        let mut replies = buffer(self.config.reply_bytes)?;
        let mut len = 0;
        // SAFETY: Exclusive owner, valid input slice and initialized output buffer.
        // C bounds every reply and clears the temporary pointer before returning.
        let result = unsafe {
            ffi::rt_feed(
                self.raw.as_ptr(),
                bytes.as_ptr(),
                bytes.len(),
                replies.as_mut_ptr(),
                replies.len(),
                &mut len,
            )
        };
        if let Err(error) = self.mutation(result) {
            replies.fill(0);
            return Err(error);
        }
        replies.truncate(len);
        Ok(TerminalEffects(replies))
    }
    fn resize(&mut self, size: TerminalSize, generation: u64) -> Result<(), TerminalError> {
        self.healthy()?;
        if self.generation.checked_add(1) != Some(generation) {
            return Err(TerminalError::StaleControl);
        }
        if self.restoring.is_some() {
            return Err(TerminalError::HistoryIncomplete);
        }
        // SAFETY: Validated dimensions and exclusive live owner; callbacks stay in C.
        let result = unsafe { ffi::rt_resize(self.raw.as_ptr(), size.cols(), size.rows()) };
        self.mutation(result)?;
        self.generation = generation;
        self.config.size = size;
        Ok(())
    }
    fn view(&mut self) -> Result<TerminalView, TerminalError> {
        self.project()
    }
    fn checkpoint(
        &mut self,
        descriptor: CheckpointDescriptor,
    ) -> Result<TerminalCheckpoint, TerminalError> {
        self.healthy()?;
        if descriptor.compatibility != COMPATIBILITY {
            return Err(TerminalError::IncompatibleCheckpoint);
        }
        if descriptor.control_generation != self.generation {
            return Err(TerminalError::StaleControl);
        }
        if self.restoring.is_some() {
            return Err(TerminalError::HistoryIncomplete);
        }
        let mut bytes = buffer(self.config.checkpoint_bytes)?;
        let mut len = 0;
        // SAFETY: Exclusive live owner and initialized bounded byte output. C writer
        // never grows the buffer or stores its pointer beyond this synchronous call.
        let result = unsafe {
            ffi::rt_checkpoint(self.raw.as_ptr(), bytes.as_mut_ptr(), bytes.len(), &mut len)
        };
        if result != 0 {
            bytes.fill(0);
            return Err(if result == -2 {
                TerminalError::BudgetExceeded
            } else {
                TerminalError::EngineFailure
            });
        }
        bytes.truncate(len);
        Ok(TerminalCheckpoint { descriptor, bytes })
    }
    fn restoration_progress(&self) -> RestorationProgress {
        if self.restoring.is_some() {
            RestorationProgress::Usable
        } else {
            RestorationProgress::Complete
        }
    }
    fn restore_history_step(&mut self) -> Result<RestorationProgress, TerminalError> {
        self.healthy()?;
        // SAFETY: Exclusive owner retains checkpoint bytes; C decoder performs one
        // history unit and relinquishes its borrowed source only when complete.
        let result = unsafe { ffi::rt_history(self.raw.as_ptr()) };
        match result {
            1 => {
                self.restoring = None;
                Ok(RestorationProgress::Complete)
            }
            0 => Ok(RestorationProgress::Usable),
            _ => {
                self.failed = true;
                Err(if result == -2 {
                    TerminalError::BudgetExceeded
                } else {
                    TerminalError::CorruptCheckpoint
                })
            }
        }
    }
    fn compress_history_step(&mut self) -> Result<bool, TerminalError> {
        self.healthy()?;
        if self.restoring.is_some() {
            return Err(TerminalError::HistoryIncomplete);
        }
        // SAFETY: Exclusive live owner; no other call may access pages during compression.
        match unsafe { ffi::rt_compress(self.raw.as_ptr()) } {
            0 => Ok(false),
            1 => Ok(true),
            -3 => Err(TerminalError::Unsupported),
            result => {
                self.mutation(result)?;
                Err(TerminalError::EngineFailure)
            }
        }
    }
}
