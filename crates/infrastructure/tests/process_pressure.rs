//! Actual concurrency, bounded admission, failure containment and launch-path checks.
#[path = "fixtures/process_support.rs"]
mod support;
use pty_runtime_application::process::{IProcessBackend, IProcessEvents, OutputAcceptance};
use pty_runtime_domain::{
    SessionLifetime,
    process::{CommandSpec, DrainOutcome, ExitStatus, ProcessError, ProcessLimits},
};
use pty_runtime_infrastructure::process::UnixProcessBackend;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use support::*;
#[test]
fn simultaneous_producers_deliver_complete_bytes_and_quiet_cancel_remains_live() {
    let owner = backend(17);
    let quiet = Arc::new(Events::default());
    let quiet_session = owner
        .spawn(
            &command("printf ready; sleep 60"),
            size(),
            SessionLifetime::new(2, 1),
            ProcessLimits::default(),
            quiet.clone(),
        )
        .unwrap();
    quiet.wait(|s| s.bytes == b"ready");
    let mut producers = Vec::new();
    for index in 0..16 {
        let events = Arc::new(Events::default());
        owner
            .spawn(
                &command("dd if=/dev/zero bs=4096 count=32 2>/dev/null"),
                size(),
                SessionLifetime::new(2, index + 2),
                ProcessLimits::default(),
                events.clone(),
            )
            .unwrap();
        producers.push(events);
    }
    quiet_session.request_cancel().unwrap();
    quiet.wait(|s| s.exit.is_some());
    for events in producers {
        events.wait(|s| s.exit.is_some() && s.drain.is_some());
        let state = events.state.lock().unwrap();
        assert_eq!(state.exit, Some(ExitStatus::Code(0)));
        assert_eq!(state.drain, Some(DrainOutcome::Eof));
        assert_eq!(state.bytes.len(), 131072);
        assert!(state.bytes.iter().all(|byte| *byte == 0));
    }
    owner.shutdown();
}
#[test]
fn stalled_input_times_out_with_partial_ack_and_retains_no_admission() {
    let owner = backend(1);
    let events = Arc::new(Events::default());
    let limits = ProcessLimits {
        input_slots: 1,
        input_chunk: 131072,
        input_bytes: 131072,
        write_timeout: Duration::from_millis(40),
        ..ProcessLimits::default()
    };
    let session = owner
        .spawn(
            &command("stty -icanon min 1 time 0; printf ready; sleep 60"),
            size(),
            SessionLifetime::new(3, 1),
            limits,
            events.clone(),
        )
        .unwrap();
    events.wait(|s| s.bytes == b"ready");
    let result = wait(session.write(&vec![b'x'; 131072]).unwrap());
    assert_eq!(result.error, Some(ProcessError::Timeout));
    assert!(result.written < 131072);
    // The timed-out chunk no longer owns its slot, even when its future was held.
    let next = session.write(b"next").unwrap();
    session.request_cancel().unwrap();
    assert!(wait(next).written <= 4);
    owner.shutdown();
}
struct Panics;
impl IProcessEvents for Panics {
    fn output(&self, _: &[u8]) -> OutputAcceptance {
        panic!("synthetic callback failure")
    }
    fn wait_for_capacity(&self, _: Instant) {}
    fn exited(&self, _: ExitStatus) {
        panic!("synthetic exit callback failure")
    }
    fn drained(&self, _: DrainOutcome) {
        panic!("synthetic drain callback failure")
    }
    fn supervision_failed(&self, _: ProcessError) {}
}
#[test]
fn callback_panics_do_not_orphan_children_or_break_other_sessions() {
    let owner = backend(2);
    owner
        .spawn(
            &command("printf output; sleep 1"),
            size(),
            SessionLifetime::new(4, 1),
            ProcessLimits::default(),
            Arc::new(Panics),
        )
        .unwrap();
    let events = Arc::new(Events::default());
    owner
        .spawn(
            &command("printf healthy; exit 0"),
            size(),
            SessionLifetime::new(4, 2),
            ProcessLimits::default(),
            events.clone(),
        )
        .unwrap();
    events.wait(|s| s.exit.is_some() && s.drain.is_some());
    assert_eq!(events.state.lock().unwrap().bytes, b"healthy");
    owner.shutdown();
}
#[test]
fn symlinks_cannot_escape_allowed_launch_roots() {
    use std::os::unix::fs::symlink;
    let root = std::env::temp_dir().join(format!("pty-path-fixture-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let link = root.join("outside");
    symlink("/", &link).unwrap();
    let owner = UnixProcessBackend::new(vec![root.clone()], 1).unwrap();
    let spec =
        CommandSpec::new("/bin/sh".into(), link, vec!["-c".into(), "exit 0".into()]).unwrap();
    assert!(matches!(
        owner.spawn(
            &spec,
            size(),
            SessionLifetime::new(5, 1),
            ProcessLimits::default(),
            Arc::new(Events::default())
        ),
        Err(ProcessError::OutsideRoots)
    ));
    owner.shutdown();
    std::fs::remove_dir_all(root).unwrap();
}
