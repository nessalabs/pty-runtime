//! Deterministic real Ghostty/AEAD/disk/coordinator restoration; process events are injected.
#![cfg(feature = "ghostty")]
#[allow(dead_code)]
mod support;
use pty_runtime::{ports::*, terminal::*, *};
use pty_runtime_infrastructure::{
    checkpoint::{CheckpointProtector, FileCheckpointStore},
    registry::MemorySessionRepository,
    scheduling::*,
    terminal::GhosttyTerminalFactory,
};
use std::{
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Default)]
struct Gate {
    entered: AtomicBool,
    released: Mutex<bool>,
    changed: Condvar,
}
impl Gate {
    fn release(&self) {
        *self.released.lock().unwrap() = true;
        self.changed.notify_all();
    }
    fn pause(&self) -> Result<(), TerminalError> {
        if self.entered.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        let released = self.released.lock().unwrap();
        let (released, _) = self
            .changed
            .wait_timeout_while(released, Duration::from_secs(5), |v| !*v)
            .unwrap();
        if *released {
            Ok(())
        } else {
            Err(TerminalError::EngineFailure)
        }
    }
}
struct Release(Arc<Gate>);
impl Drop for Release {
    fn drop(&mut self) {
        self.0.release();
    }
}
struct Factory(Arc<Gate>);
impl ITerminalFactory for Factory {
    fn capabilities(&self) -> TerminalCapabilities {
        GhosttyTerminalFactory.capabilities()
    }
    fn compatibility(&self) -> &'static str {
        GhosttyTerminalFactory.compatibility()
    }
    fn create(&self, c: TerminalConfig) -> Result<Box<dyn ITerminal>, TerminalError> {
        GhosttyTerminalFactory.create(c)
    }
    fn restore(
        &self,
        p: TerminalCheckpoint,
        c: TerminalConfig,
    ) -> Result<Box<dyn ITerminal>, TerminalError> {
        Ok(Box::new(Terminal(
            GhosttyTerminalFactory.restore(p, c)?,
            self.0.clone(),
        )))
    }
}
struct Terminal(Box<dyn ITerminal>, Arc<Gate>);
impl ITerminal for Terminal {
    fn history(&mut self, start: u64, _count: u16) -> Result<TerminalHistory, TerminalError> {
        // This double drives restoration ordering, not retained rows.
        Ok(TerminalHistory {
            start,
            cols: 0,
            cells: Vec::new(),
            total: 0,
            scrollback: 0,
        })
    }

    fn feed(&mut self, b: &[u8]) -> Result<TerminalEffects, TerminalError> {
        self.0.feed(b)
    }
    fn resize(&mut self, s: TerminalSize, g: ControlGeneration) -> Result<(), TerminalError> {
        self.0.resize(s, g)
    }
    fn view(&mut self) -> Result<TerminalView, TerminalError> {
        self.0.view()
    }
    fn checkpoint(&mut self, d: CheckpointDescriptor) -> Result<TerminalCheckpoint, TerminalError> {
        self.0.checkpoint(d)
    }
    fn restoration_progress(&self) -> RestorationProgress {
        self.0.restoration_progress()
    }
    fn restore_history_step(&mut self) -> Result<RestorationProgress, TerminalError> {
        self.1.pause()?;
        self.0.restore_history_step()
    }
    fn compress_history_step(&mut self) -> Result<bool, TerminalError> {
        self.0.compress_history_step()
    }
}
#[derive(Default)]
struct Backend(Mutex<Option<Arc<dyn IProcessEvents>>>);
struct Process;
impl IProcessSession for Process {
    fn process_id(&self) -> u32 {
        1
    }
    fn write_reserved(
        &self,
        b: &[u8],
        _: Option<Box<dyn IInputReservation>>,
    ) -> Result<ProcessOperation<WriteOutcome>, ProcessError> {
        let written = b.len();
        Ok(Box::pin(async move {
            WriteOutcome {
                written,
                error: None,
            }
        }))
    }
    fn request_cancel(&self) -> Result<(), ProcessError> {
        Ok(())
    }
    fn resize(
        &self,
        _: TerminalSize,
    ) -> Result<ProcessOperation<Result<(), ProcessError>>, ProcessError> {
        Ok(Box::pin(async { Ok(()) }))
    }
}
impl IProcessBackend for Backend {
    fn spawn(
        &self,
        _: &CommandSpec,
        _: TerminalSize,
        _: SessionLifetime,
        _: ProcessLimits,
        e: Arc<dyn IProcessEvents>,
    ) -> Result<Arc<dyn IProcessSession>, ProcessError> {
        *self.0.lock().unwrap() = Some(e);
        Ok(Arc::new(Process))
    }
    fn shutdown(&self) {}
    fn shutdown_now(&self) {}
}
fn until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}
unsafe extern "C" {
    fn rt_verify_format(
        bytes: *const u8,
        len: usize,
        out: *mut u8,
        cap: usize,
        written: *mut usize,
    ) -> i32;
}
fn canonical(checkpoint: &TerminalCheckpoint) -> Vec<u8> {
    let mut bytes = vec![0; 1024 * 1024];
    let mut len = 0;
    // SAFETY: immutable source and initialized bounded destination remain live for this synchronous independent oracle.
    assert_eq!(
        unsafe {
            rt_verify_format(
                checkpoint.bytes.as_ptr(),
                checkpoint.bytes.len(),
                bytes.as_mut_ptr(),
                bytes.len(),
                &mut len,
            )
        },
        0
    );
    bytes.truncate(len);
    bytes
}
#[test]
fn ready_applies_exact_64_byte_suffix_before_history_and_retains_encrypted_source() {
    let gate = Arc::new(Gate::default());
    let store = Arc::new(FileCheckpointStore::temporary(None, 32 * 1024 * 1024).unwrap());
    let backend = Arc::new(Backend::default());
    let runtime = Runtime::with_projection_adapters(
        RuntimeOptions::default(),
        Arc::new(MemorySessionRepository::default()),
        backend.clone(),
        ProjectionServices {
            terminal: Arc::new(Factory(gate.clone())),
            clock: Arc::new(MonotonicSystemClock::default()),
            scheduler: Arc::new(StdWorkScheduler::with_workers(2, 1).unwrap()),
            blocking: Arc::new(BoundedBlockingExecutor::new(2, 1).unwrap()),
            capacity: Arc::new(CondvarCapacitySignal::default()),
            store: store.clone(),
            protector: Arc::new(CheckpointProtector::new(8 * 1024 * 1024).unwrap()),
        },
    )
    .unwrap();
    let _release = Release(gate.clone());
    let config = TerminalConfig::new(TerminalSize::new(80, 24).unwrap());
    let mut options = SessionOptions::projected(config);
    options.projection.as_mut().unwrap().park_after = Duration::from_millis(50);
    let session = runtime
        .spawn(
            support::id("ready-real"),
            &support::command("echo", &[]),
            options,
        )
        .unwrap();
    let events = backend.0.lock().unwrap().as_ref().unwrap().clone();
    let before: Vec<u8> = b"x\r\n".iter().copied().cycle().take(8192).collect();
    for chunk in before.chunks(4096) {
        assert_eq!(events.output(chunk), OutputAcceptance::Accepted);
    }
    until(|| session.projection_status().unwrap().unwrap().residency == Residency::Parked);
    let pin = support::block_on(session.terminal_checkpoint().unwrap()).unwrap();
    assert_eq!(pin.checkpoint().descriptor.processed.offset, 8192);
    let mut reference = GhosttyTerminalFactory
        .restore(pin.checkpoint().clone(), config)
        .unwrap();
    while !reference.restoration_progress().is_finished() {
        reference.restore_history_step().unwrap();
    }
    assert_eq!(
        reference.restoration_progress(),
        RestorationProgress::Complete
    );
    drop(pin);
    let suffix = [b'L'; 64];
    assert_eq!(events.output(&suffix), OutputAcceptance::Accepted);
    until(|| gate.entered.load(Ordering::Acquire));
    let status = session.projection_status().unwrap().unwrap();
    let source_bytes = store.capacity().committed_bytes;
    gate.release();
    assert_eq!(status.processed.offset, 8256);
    assert_eq!(status.published.offset, 8256);
    assert_eq!(status.history, RestorationProgress::Usable);
    assert!(
        source_bytes > 0,
        "source was deleted before its history was validated"
    );
    reference.feed(&suffix).unwrap();
    let pin = support::block_on(session.terminal_checkpoint().unwrap()).unwrap();
    assert_eq!(pin.checkpoint().descriptor.processed.offset, 8256);
    let expected = reference
        .checkpoint(pin.checkpoint().descriptor.clone())
        .unwrap();
    assert_eq!(canonical(pin.checkpoint()), canonical(&expected));
    assert_eq!(
        session.projection_status().unwrap().unwrap().history,
        RestorationProgress::Complete
    );
    runtime.shutdown();
    assert_eq!(store.capacity().committed_bytes, 0);
}
