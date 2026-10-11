//! A delayed accepting reader must distinguish actual EOF from its drain deadline.
#[path = "fixtures/process_support.rs"]
mod support;
use pty_runtime_application::process::{IProcessBackend, IProcessEvents, OutputAcceptance};
use pty_runtime_domain::{
    SessionLifetime,
    process::{CommandSpec, DrainOutcome, ExitStatus, ProcessError, ProcessLimits},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use support::*;

#[derive(Default)]
struct DelayedEvents {
    events: Events,
    delayed: AtomicBool,
    callbacks: AtomicUsize,
}
impl IProcessEvents for DelayedEvents {
    fn output(&self, bytes: &[u8]) -> OutputAcceptance {
        self.callbacks.fetch_add(1, Ordering::SeqCst);
        if !self.delayed.swap(true, Ordering::SeqCst) {
            // The exit callback runs independently of this reader. Starting the
            // delay after that observation makes the 250 ms cutoff intentional.
            self.events.wait(|state| state.exit.is_some());
            std::thread::sleep(Duration::from_millis(350));
        }
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
fn collect(drain_timeout: Duration, expected: DrainOutcome) {
    let owner = backend(1);
    let events = Arc::new(DelayedEvents::default());
    let _session = owner
        .spawn(
            &command("printf literal; exit 0"),
            size(),
            SessionLifetime::new(107, 1),
            ProcessLimits {
                drain_timeout,
                ..ProcessLimits::default()
            },
            events.clone(),
        )
        .unwrap();
    events
        .events
        .wait(|state| state.exit.is_some() && state.drain.is_some());
    {
        let state = events.events.state.lock().unwrap();
        assert_eq!(state.exit, Some(ExitStatus::Code(0)));
        assert_eq!(state.bytes, b"literal");
        assert_eq!(state.drain, Some(expected));
    }
    assert!(events.delayed.load(Ordering::SeqCst));
    assert!(events.callbacks.load(Ordering::SeqCst) >= 1);
    owner.shutdown();
}
#[test]
fn delayed_acceptance_reaches_eof_after_workload_exit_with_retained_endpoint() {
    collect(Duration::from_secs(2), DrainOutcome::Eof);
}
#[test]
fn delayed_acceptance_preserves_an_expired_drain_deadline() {
    collect(Duration::from_millis(250), DrainOutcome::Truncated);
}

struct ReleaseDirectory(std::path::PathBuf);
impl Drop for ReleaseDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[test]
fn descendant_output_after_observed_workload_exit_is_delivered_before_eof() {
    let directory = std::env::temp_dir().join(format!("pty-late-output-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let directory = ReleaseDirectory(directory);
    let release = directory.0.join("release");
    let owner = backend(1);
    let events = Arc::new(Events::default());
    // The descendant cannot write its tail before this test has observed exit.
    // Its bounded polling also lets it terminate if the test fails beforehand.
    let spec = CommandSpec::new(
        "/bin/sh".into(),
        std::env::temp_dir(),
        vec![
            "-c".into(),
            "trap '' HUP; printf initial; (n=0; while [ ! -e \"$1\" ]; do n=$((n+1)); [ \"$n\" -lt 300 ] || exit 91; sleep 0.01; done; printf tail) & exit 23".into(),
            "late-output".into(),
            release.into_os_string(),
        ],
    )
    .unwrap();
    owner
        .spawn(
            &spec,
            size(),
            SessionLifetime::new(108, 1),
            ProcessLimits {
                drain_timeout: Duration::from_secs(2),
                ..ProcessLimits::default()
            },
            events.clone(),
        )
        .unwrap();
    events.wait(|state| state.exit.is_some());
    assert_eq!(events.state.lock().unwrap().drain, None);
    std::fs::write(directory.0.join("release"), []).unwrap();
    events.wait(|state| state.drain.is_some());
    {
        let state = events.state.lock().unwrap();
        assert_eq!(state.exit, Some(ExitStatus::Code(23)));
        assert_eq!(state.bytes, b"initialtail");
        assert_eq!(state.drain, Some(DrainOutcome::Eof));
    }
    owner.shutdown();
}
