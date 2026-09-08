//! Host signal policy rejection and immediate cleanup are exercised in real processes.
#[path = "fixtures/process_support.rs"]
mod support;
use pty_runtime_application::process::{IProcessBackend, IProcessEvents, OutputAcceptance};
use pty_runtime_domain::{
    SessionLifetime,
    process::{DrainOutcome, ExitStatus, ProcessError, ProcessLimits},
};
use pty_runtime_infrastructure::process::UnixProcessBackend;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use support::*;
#[test]
fn constructor_rejects_auto_reaping_policies_in_isolated_hosts() {
    for policy in ["ignore", "no_wait"] {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "sigchld_policy_fixture",
                "--ignored",
                "--nocapture",
            ])
            .env("PTY_SIGNAL_POLICY", policy)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}
#[test]
#[ignore = "runs only inside the isolated host-policy subprocess"]
fn sigchld_policy_fixture() {
    let policy = std::env::var("PTY_SIGNAL_POLICY").unwrap();
    // SAFETY: this isolated fixture process has no managed children or adapter workers.
    let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
    action.sa_sigaction = if policy == "ignore" {
        libc::SIG_IGN
    } else {
        libc::SIG_DFL
    };
    if policy == "no_wait" {
        action.sa_flags = libc::SA_NOCLDWAIT;
    }
    // SAFETY: valid signal and initialized action; fixture exits without further children.
    assert_eq!(
        unsafe { libc::sigaction(libc::SIGCHLD, &action, std::ptr::null_mut()) },
        0
    );
    assert!(matches!(
        UnixProcessBackend::new(vec![std::env::temp_dir()], 1),
        Err(ProcessError::Unsupported)
    ));
}
#[test]
fn backend_drop_ignores_twenty_four_hour_grace_and_reaps() {
    let owner = backend(1);
    let events = Arc::new(Events::default());
    let session = owner
        .spawn(
            &command("trap '' TERM; printf ready; sleep 60"),
            size(),
            SessionLifetime::new(91, 1),
            ProcessLimits {
                terminate_grace: Duration::from_secs(86400),
                ..ProcessLimits::default()
            },
            events.clone(),
        )
        .unwrap();
    events.wait(|s| s.bytes == b"ready");
    let now = Instant::now();
    drop(owner);
    assert!(now.elapsed() < Duration::from_secs(2));
    assert_eq!(
        events.state.lock().unwrap().exit,
        Some(ExitStatus::Signal(libc::SIGKILL))
    );
    // SAFETY: this fixture only inspects its child; backend must have already reaped it.
    assert_eq!(
        unsafe {
            libc::waitpid(
                session.process_id() as _,
                std::ptr::null_mut(),
                libc::WNOHANG,
            )
        },
        -1
    );
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ECHILD)
    );
}
struct BrokenReader(Events);
impl IProcessEvents for BrokenReader {
    fn output(&self, _: &[u8]) -> OutputAcceptance {
        panic!("synthetic reader failure")
    }
    fn wait_for_capacity(&self, _: Instant) {}
    fn exited(&self, status: ExitStatus) {
        self.0.exited(status);
    }
    fn drained(&self, outcome: DrainOutcome) {
        self.0.drained(outcome);
    }
    fn supervision_failed(&self, error: ProcessError) {
        self.0.supervision_failed(error);
    }
}
#[test]
fn unexpected_reader_failure_kills_long_lived_child_without_grace_delay() {
    let owner = backend(1);
    let events = Arc::new(BrokenReader(Events::default()));
    owner
        .spawn(
            &command("trap '' TERM; printf output; sleep 60"),
            size(),
            SessionLifetime::new(92, 1),
            ProcessLimits {
                terminate_grace: Duration::from_secs(86400),
                ..ProcessLimits::default()
            },
            events.clone(),
        )
        .unwrap();
    events.0.wait(|s| s.exit.is_some() && s.drain.is_some());
    let state = events.0.state.lock().unwrap();
    assert_eq!(state.exit, Some(ExitStatus::Signal(libc::SIGKILL)));
    assert_eq!(
        state.drain,
        Some(DrainOutcome::Failed(ProcessError::Internal))
    );
    drop(state);
    owner.shutdown_now();
}
