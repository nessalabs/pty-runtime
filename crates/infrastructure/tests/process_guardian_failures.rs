//! The public adapter distinguishes helper loss from the actual workload status.
#[path = "fixtures/process_support.rs"]
mod support;
use pty_runtime_application::process::IProcessBackend;
use pty_runtime_domain::{
    SessionLifetime,
    process::{ProcessError, ProcessLimits},
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[test]
fn guardian_loss_reports_supervision_failure_without_fabricated_workload_exit() {
    damage(false);
}
#[test]
fn sentinel_loss_preserves_cleanup_and_reports_supervision_failure() {
    damage(true);
}
fn damage(sentinel: bool) {
    let owner = support::backend(1);
    let events = Arc::new(support::Events::default());
    let session = owner
        .spawn(
            &support::command(
                "trap '' HUP TERM; printf '%s %s\\n' \"$$\" \"$PPID\"; sleep 300 & wait",
            ),
            support::size(),
            SessionLifetime::new(701, 1),
            ProcessLimits::default(),
            events.clone(),
        )
        .unwrap();
    events.wait(|state| state.bytes.contains(&b'\n'));
    let bytes = events.state.lock().unwrap().bytes.clone();
    let ids: Vec<i32> = std::str::from_utf8(&bytes)
        .unwrap()
        .split_whitespace()
        .map(|value| value.parse().unwrap())
        .collect();
    // SAFETY: query the live fixture's session and damage only its owned helper.
    let sid = unsafe { libc::getsid(ids[0]) };
    assert!(sid > 0);
    let target = if sentinel { sid } else { ids[1] };
    assert_eq!(unsafe { libc::kill(target, libc::SIGKILL) }, 0);
    let deadline = Instant::now() + Duration::from_secs(3);
    while events.state.lock().unwrap().failure.is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    owner.shutdown();
    let state = events.state.lock().unwrap();
    assert_eq!(state.failure, Some(ProcessError::Internal));
    if !sentinel {
        assert_eq!(state.exit, None);
    }
    assert!(
        Instant::now() < deadline,
        "helper recovery exceeded deadline"
    );
    drop(state);
    assert!(matches!(
        session.resize(support::size()),
        Err(ProcessError::Closed)
    ));
}
