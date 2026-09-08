//! Adversarial reader failure must not leave an undrainable live child behind.
#[path = "fixtures/process_support.rs"]
mod support;
use pty_runtime_application::process::{IProcessBackend, IProcessEvents, OutputAcceptance};
use pty_runtime_domain::{
    SessionLifetime,
    process::{DrainOutcome, ExitStatus, ProcessError, ProcessLimits},
};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

#[derive(Default)]
struct FailedReader {
    state: Mutex<(bool, bool)>,
    changed: Condvar,
}
impl IProcessEvents for FailedReader {
    fn output(&self, _: &[u8]) -> OutputAcceptance {
        panic!("synthetic reader delivery failure")
    }
    fn wait_for_capacity(&self, _: Instant) {}
    fn exited(&self, _: ExitStatus) {
        self.state.lock().unwrap().1 = true;
        self.changed.notify_all();
    }
    fn drained(&self, outcome: DrainOutcome) {
        assert_eq!(outcome, DrainOutcome::Failed(ProcessError::Internal));
        self.state.lock().unwrap().0 = true;
        self.changed.notify_all();
    }
    fn supervision_failed(&self, _: ProcessError) {}
}

#[test]
fn failed_reader_terminates_long_lived_child_without_owner_shutdown() {
    let owner = support::backend(1);
    let events = Arc::new(FailedReader::default());
    let _session = owner
        .spawn(
            &support::command("printf trigger; sleep 60"),
            support::size(),
            SessionLifetime::new(72, 1),
            ProcessLimits::default(),
            events.clone(),
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut state = events.state.lock().unwrap();
    while !state.1 && Instant::now() < deadline {
        state = events
            .changed
            .wait_timeout(state, deadline.saturating_duration_since(Instant::now()))
            .unwrap()
            .0;
    }
    let observed_failure = state.0;
    let reaped_without_shutdown = state.1;
    drop(state);
    // Always clean up first, including when this regression catches the defect.
    owner.shutdown();
    assert!(
        observed_failure,
        "reader did not reach the injected failure"
    );
    assert!(
        reaped_without_shutdown,
        "failed reader left the long-lived process running until owner shutdown"
    );
}
