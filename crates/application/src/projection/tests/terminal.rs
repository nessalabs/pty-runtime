use crate::terminal::{ITerminal, ITerminalFactory};
use pty_runtime_domain::terminal::*;
use std::sync::{
    Arc, Barrier, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trace {
    Feed(Vec<u8>),
    Resize(u64),
    Encode(u64, u64),
    Restore(u64, u64),
    History(usize),
}
#[derive(Default)]
pub struct Probe {
    pub trace: Mutex<Vec<Trace>>,
    pub alive: AtomicUsize,
    pub encode_barrier: Mutex<Option<Arc<Barrier>>>,
    pub fail_restore: AtomicBool,
    pub panic_feed: AtomicBool,
    pub fail_resize: AtomicBool,
    pub live_restore: AtomicBool,
    pub skip_history: AtomicBool,
    pub fail_history: AtomicBool,
}
pub struct Factory(pub Arc<Probe>);
impl ITerminalFactory for Factory {
    fn capabilities(&self) -> TerminalCapabilities {
        TerminalCapabilities {
            checkpoints: true,
            incremental_restore: true,
            mutation_during_restore: self.0.live_restore.load(Ordering::Acquire),
            history_compression: false,
        }
    }
    fn compatibility(&self) -> &'static str {
        "test-native-v1"
    }
    fn create(&self, config: TerminalConfig) -> Result<Box<dyn ITerminal>, TerminalError> {
        self.0.alive.fetch_add(1, Ordering::AcqRel);
        Ok(Box::new(Terminal {
            probe: self.0.clone(),
            bytes: Vec::new(),
            size: config.size,
            generation: 0,
            history: 0,
            skipped: 0,
        }))
    }
    fn restore(
        &self,
        checkpoint: TerminalCheckpoint,
        config: TerminalConfig,
    ) -> Result<Box<dyn ITerminal>, TerminalError> {
        if self.0.fail_restore.load(Ordering::Acquire) {
            return Err(TerminalError::CorruptCheckpoint);
        }
        self.0.trace.lock().unwrap().push(Trace::Restore(
            checkpoint.descriptor.processed.offset,
            checkpoint.descriptor.control_generation,
        ));
        self.0.alive.fetch_add(1, Ordering::AcqRel);
        Ok(Box::new(Terminal {
            probe: self.0.clone(),
            bytes: checkpoint.bytes.clone(),
            size: config.size,
            generation: checkpoint.descriptor.control_generation,
            history: 2,
            skipped: 0,
        }))
    }
}
struct Terminal {
    probe: Arc<Probe>,
    bytes: Vec<u8>,
    size: TerminalSize,
    generation: u64,
    history: usize,
    skipped: u64,
}
impl Drop for Terminal {
    fn drop(&mut self) {
        self.probe.alive.fetch_sub(1, Ordering::AcqRel);
    }
}
impl ITerminal for Terminal {
    fn feed(&mut self, bytes: &[u8]) -> Result<TerminalEffects, TerminalError> {
        assert!(
            !self.probe.panic_feed.load(Ordering::Acquire),
            "injected native panic"
        );
        assert!(self.history == 0 || self.probe.live_restore.load(Ordering::Acquire));
        self.probe
            .trace
            .lock()
            .unwrap()
            .push(Trace::Feed(bytes.to_vec()));
        self.bytes.extend_from_slice(bytes);
        Ok(TerminalEffects(if bytes.contains(&b'?') {
            b"R".to_vec()
        } else {
            Vec::new()
        }))
    }
    fn resize(&mut self, size: TerminalSize, generation: u64) -> Result<(), TerminalError> {
        assert!(self.history == 0 || self.probe.live_restore.load(Ordering::Acquire));
        if self.probe.fail_resize.load(Ordering::Acquire) {
            return Err(TerminalError::EngineFailure);
        }
        assert_eq!(generation, self.generation + 1);
        self.generation = generation;
        self.size = size;
        self.probe
            .trace
            .lock()
            .unwrap()
            .push(Trace::Resize(generation));
        Ok(())
    }
    fn view(&mut self) -> Result<TerminalView, TerminalError> {
        let style = TerminalStyle {
            foreground: TerminalColor::Default,
            background: TerminalColor::Default,
            underline_color: TerminalColor::Default,
            underline: Underline::None,
            bold: false,
            italic: false,
            faint: false,
            blink: false,
            inverse: false,
            invisible: false,
            strikethrough: false,
            overline: false,
        };
        let mut cells = vec![
            TerminalCell {
                text: String::new(),
                width: 1,
                style
            };
            usize::from(self.size.cols()) * usize::from(self.size.rows())
        ];
        cells[0].text = String::from_utf8_lossy(&self.bytes).into();
        Ok(TerminalView {
            size: self.size,
            cursor: TerminalCursor {
                col: 0,
                row: 0,
                visible: true,
                pending_wrap: false,
            },
            modes: TerminalModes {
                alternate_screen: false,
                bracketed_paste: false,
                application_cursor: false,
                mouse_reporting: false,
            },
            palette: TerminalPalette {
                foreground: None,
                background: None,
                cursor: None,
                indexed: [[0; 3]; 256],
            },
            cells,
        })
    }
    fn checkpoint(
        &mut self,
        descriptor: CheckpointDescriptor,
    ) -> Result<TerminalCheckpoint, TerminalError> {
        self.probe.trace.lock().unwrap().push(Trace::Encode(
            descriptor.processed.offset,
            descriptor.control_generation,
        ));
        let barrier = self.probe.encode_barrier.lock().unwrap().take();
        if let Some(barrier) = barrier {
            barrier.wait();
            barrier.wait();
        }
        Ok(TerminalCheckpoint {
            descriptor,
            bytes: self.bytes.clone(),
        })
    }
    fn restoration_progress(&self) -> RestorationProgress {
        match (self.history, self.skipped) {
            (0, 0) => RestorationProgress::Complete,
            (_, 0) => RestorationProgress::Usable,
            (0, skipped_pages) => RestorationProgress::FinishedWithSkippedHistory { skipped_pages },
            (_, skipped_pages) => RestorationProgress::UsableWithSkippedHistory { skipped_pages },
        }
    }
    fn restore_history_step(&mut self) -> Result<RestorationProgress, TerminalError> {
        if self.probe.fail_history.load(Ordering::Acquire) {
            return Err(TerminalError::CorruptCheckpoint);
        }
        if self.history > 0 && self.probe.skip_history.load(Ordering::Acquire) {
            self.skipped += 1;
        }
        self.history = self.history.saturating_sub(1);
        self.probe
            .trace
            .lock()
            .unwrap()
            .push(Trace::History(self.history));
        Ok(self.restoration_progress())
    }

    fn compress_history_step(&mut self) -> Result<bool, TerminalError> {
        Ok(true)
    }
}
