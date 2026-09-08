//! Reader memory gauges observe real allocations rather than configured session estimates.
#[path = "fixtures/process_events.rs"]
mod support;
use pty_runtime_application::{
    diagnostics::RuntimeDiagnostics,
    process::{IProcessBackend, IProcessEvents, OutputAcceptance},
};
use pty_runtime_domain::{
    SessionLifetime,
    process::{DrainOutcome, ExitStatus, ProcessError, ProcessLimits},
};
use pty_runtime_infrastructure::process::UnixProcessBackend;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

struct Measured {
    events: support::Events,
    diagnostics: Arc<RuntimeDiagnostics>,
    panic_output: bool,
}
impl IProcessEvents for Measured {
    fn diagnostics(&self) -> Option<Arc<RuntimeDiagnostics>> {
        Some(self.diagnostics.clone())
    }
    fn output(&self, bytes: &[u8]) -> OutputAcceptance {
        assert!(!self.panic_output, "synthetic output observer panic");
        self.events.output(bytes)
    }
    fn wait_for_capacity(&self, deadline: Instant) {
        self.events.wait_for_capacity(deadline);
    }
    fn exited(&self, status: ExitStatus) {
        self.events.exited(status);
    }
    fn drained(&self, outcome: DrainOutcome) {
        self.events.drained(outcome);
    }
    fn supervision_failed(&self, error: ProcessError) {
        self.events.supervision_failed(error);
    }
}
fn counts(diagnostics: &RuntimeDiagnostics) -> (u64, u64) {
    let snapshot = diagnostics.aggregate();
    (
        snapshot.live_readers,
        snapshot.reader_scratch_allocated_bytes,
    )
}
fn settled(diagnostics: &RuntimeDiagnostics, expected: (u64, u64)) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while counts(diagnostics) != expected {
        assert!(
            Instant::now() < deadline,
            "reader gauges did not settle: {:?}",
            counts(diagnostics)
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn actual_readers_count_allocated_scratch_across_reset_and_independent_exit() {
    let diagnostics = RuntimeDiagnostics::new();
    let backend = UnixProcessBackend::new(vec![std::env::temp_dir()], 2).unwrap();
    assert_eq!(counts(&diagnostics), (0, 0));
    let mut sessions = Vec::new();
    for (index, bytes) in [4093, 8191].into_iter().enumerate() {
        let events = Arc::new(Measured {
            events: support::Events::default(),
            diagnostics: diagnostics.clone(),
            panic_output: false,
        });
        let limits = ProcessLimits {
            read_chunk: bytes,
            ..ProcessLimits::default()
        };
        let session = backend
            .spawn(
                &support::command(
                    "stty raw -echo; printf R; dd bs=1 count=1 of=/dev/null 2>/dev/null",
                ),
                support::size(),
                SessionLifetime::new(813, index as u64 + 1),
                limits,
                events.clone(),
            )
            .unwrap();
        events.events.wait(|state| state.bytes == b"R");
        sessions.push((session, events));
    }
    assert_eq!(counts(&diagnostics), (2, 4093 + 8191));
    diagnostics.reset_quiescent();
    assert_eq!(counts(&diagnostics), (2, 4093 + 8191));
    let written = support::wait(sessions[0].0.write_reserved(b"x", None).unwrap());
    assert_eq!(written.written, 1);
    assert_eq!(written.error, None);
    sessions[0]
        .1
        .events
        .wait(|state| state.exit.is_some() && state.drain.is_some());
    settled(&diagnostics, (1, 8191));
    // Retaining a completed process/event handle must not retain reader memory.
    assert_eq!(
        sessions[0].1.events.state.lock().unwrap().drain,
        Some(DrainOutcome::Eof)
    );
    backend.shutdown();
    assert_eq!(counts(&diagnostics), (0, 0));
    assert_eq!(sessions.len(), 2);
}

#[test]
fn actual_reader_caught_output_panic_releases_scratch_after_join() {
    let diagnostics = RuntimeDiagnostics::new();
    let backend = UnixProcessBackend::new(vec![std::env::temp_dir()], 1).unwrap();
    let events = Arc::new(Measured {
        events: support::Events::default(),
        diagnostics: diagnostics.clone(),
        panic_output: true,
    });
    let _session = backend
        .spawn(
            &support::command("printf trigger"),
            support::size(),
            SessionLifetime::new(814, 1),
            ProcessLimits {
                read_chunk: 4093,
                ..ProcessLimits::default()
            },
            events.clone(),
        )
        .unwrap();
    // This fixture intentionally reports reader failure, so wait on drain directly.
    let deadline = Instant::now() + Duration::from_secs(5);
    while events.events.state.lock().unwrap().drain.is_none() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    backend.shutdown();
    assert_eq!(
        events.events.state.lock().unwrap().drain,
        Some(DrainOutcome::Failed(ProcessError::Internal))
    );
    assert_eq!(counts(&diagnostics), (0, 0));
}
