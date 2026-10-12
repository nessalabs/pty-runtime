//! External terminal adapter used to expose ordering and potentially mutating failures.
use crate::terminal::{ITerminal, ITerminalFactory};
use pty_runtime_domain::terminal::*;
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct Script {
    pub feeds: Mutex<Vec<Vec<u8>>>,
    pub replies: Vec<Vec<u8>>,
    pub fail_at: Option<usize>,
}
pub struct Factory {
    pub inner: Arc<dyn ITerminalFactory>,
    pub script: Arc<Script>,
}
impl ITerminalFactory for Factory {
    fn capabilities(&self) -> TerminalCapabilities {
        self.inner.capabilities()
    }
    fn compatibility(&self) -> Result<CompatibilityId, TerminalError> {
        self.inner.compatibility()
    }
    fn create(&self, config: TerminalConfig) -> Result<Box<dyn ITerminal>, TerminalError> {
        Ok(Box::new(Terminal {
            inner: self.inner.create(config)?,
            script: self.script.clone(),
        }))
    }
    fn restore(
        &self,
        checkpoint: TerminalCheckpoint,
        config: TerminalConfig,
    ) -> Result<Box<dyn ITerminal>, TerminalError> {
        Ok(Box::new(Terminal {
            inner: self.inner.restore(checkpoint, config)?,
            script: self.script.clone(),
        }))
    }
}
struct Terminal {
    inner: Box<dyn ITerminal>,
    script: Arc<Script>,
}
impl ITerminal for Terminal {
    fn feed(&mut self, bytes: &[u8]) -> Result<TerminalEffects, TerminalError> {
        let call = {
            let mut feeds = self.script.feeds.lock().unwrap();
            feeds.push(bytes.to_vec());
            feeds.len()
        };
        // The underlying model really changes before the injected error. A retry
        // would therefore replay accepted bytes, even for HistoryIncomplete.
        self.inner.feed(bytes)?;
        if self.script.fail_at == Some(call) {
            return Err(TerminalError::HistoryIncomplete);
        }
        Ok(TerminalEffects(
            self.script
                .replies
                .get(call - 1)
                .cloned()
                .unwrap_or_default(),
        ))
    }
    fn resize(
        &mut self,
        size: TerminalSize,
        generation: ControlGeneration,
    ) -> Result<(), TerminalError> {
        self.inner.resize(size, generation)
    }
    fn view(&mut self) -> Result<TerminalView, TerminalError> {
        self.inner.view()
    }
    fn history(&mut self, start: u64, count: u16) -> Result<TerminalHistory, TerminalError> {
        self.inner.history(start, count)
    }
    fn checkpoint(
        &mut self,
        descriptor: CheckpointDescriptor,
    ) -> Result<TerminalCheckpoint, TerminalError> {
        self.inner.checkpoint(descriptor)
    }
    fn restoration_progress(&self) -> RestorationProgress {
        self.inner.restoration_progress()
    }
    fn restore_history_step(&mut self) -> Result<RestorationProgress, TerminalError> {
        self.inner.restore_history_step()
    }
    fn compress_history_step(&mut self) -> Result<bool, TerminalError> {
        self.inner.compress_history_step()
    }
}
