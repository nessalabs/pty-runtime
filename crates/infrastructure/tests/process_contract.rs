//! Actual Unix process and PTY boundary contracts.
#[path = "fixtures/process_support.rs"]
mod support;
use pty_runtime_application::process::IProcessBackend;
use pty_runtime_domain::{
    SessionLifetime,
    process::{
        CommandSpec, DrainOutcome, EnvironmentPolicy, ExitStatus, ProcessError, ProcessLimits,
    },
    terminal::TerminalSize,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use support::*;
#[test]
fn controlling_terminal_echo_off_ordered_input_and_real_exit() {
    let owner = backend(2);
    let events = Arc::new(Events::default());
    let session = owner.spawn(&command("test -t 0 && test -t 1 && test -t 2 || exit 91; test -r /dev/tty || exit 92; printf 'ready'; read x; test \"$x\" = 'synthetic-code' || exit 93; printf 'accepted'; exit 7"),size(),SessionLifetime::new(1,1),ProcessLimits::default(),events.clone()).unwrap();
    events.wait(|s| s.bytes.ends_with(b"ready"));
    let first = session.write(b"synthetic-").unwrap();
    let second = session.write(b"code\n").unwrap();
    assert_eq!(wait(first).written, 10);
    assert_eq!(wait(second).written, 5);
    events.wait(|s| s.exit.is_some() && s.drain.is_some());
    let state = events.state.lock().unwrap();
    assert_eq!(state.exit, Some(ExitStatus::Code(7)));
    assert_eq!(state.drain, Some(DrainOutcome::Eof));
    assert_eq!(state.bytes, b"readyaccepted");
    drop(state);
    owner.shutdown();
}
#[test]
fn literal_arguments_empty_environment_overrides_and_canonical_roots() {
    let owner = backend(1);
    let events = Arc::new(Events::default());
    let spec = command(
        "test -z \"${HOME+x}\" || exit 91; test \"$VALUE\" = literal || exit 92; printf clean",
    )
    .with_environment(
        EnvironmentPolicy::Empty,
        vec!["VALUE".into()],
        vec![("VALUE".into(), "literal".into())],
    )
    .unwrap();
    owner
        .spawn(
            &spec,
            size(),
            SessionLifetime::new(1, 2),
            ProcessLimits::default(),
            events.clone(),
        )
        .unwrap();
    events.wait(|s| s.exit.is_some() && s.drain.is_some());
    assert_eq!(events.state.lock().unwrap().exit, Some(ExitStatus::Code(0)));
    owner.shutdown();
    let owner = backend(1);
    let spec = CommandSpec::new("/bin/sh".into(), "/".into(), vec![]).unwrap();
    assert!(matches!(
        owner.spawn(
            &spec,
            size(),
            SessionLifetime::new(1, 3),
            ProcessLimits::default(),
            Arc::new(Events::default())
        ),
        Err(ProcessError::OutsideRoots)
    ));
}
#[test]
fn cancellation_bypasses_full_input_and_escalates_once() {
    let owner = backend(1);
    let events = Arc::new(Events::default());
    let limits = ProcessLimits {
        input_slots: 2,
        input_chunk: 65536,
        input_bytes: 131072,
        terminate_grace: Duration::from_millis(40),
        ..ProcessLimits::default()
    };
    let session = owner
        .spawn(
            &command(
                "trap '' TERM; stty -icanon min 1 time 0; printf ready; while :; do sleep 1; done",
            ),
            size(),
            SessionLifetime::new(1, 4),
            limits,
            events.clone(),
        )
        .unwrap();
    events.wait(|s| s.bytes.ends_with(b"ready"));
    let bytes = vec![b'x'; 65536];
    let first = session.write(&bytes).unwrap();
    let second = session.write(&bytes).unwrap();
    assert!(matches!(session.write(&bytes), Err(ProcessError::Capacity)));
    for _ in 0..100 {
        session.request_cancel().unwrap();
    }
    let started = Instant::now();
    events.wait(|s| s.exit.is_some() && s.drain.is_some());
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(
        events.state.lock().unwrap().exit,
        Some(ExitStatus::Signal(libc::SIGKILL))
    );
    let one = wait(first);
    let two = wait(second);
    assert!(one.written < bytes.len() || two.written < bytes.len());
    assert!(one.error.is_some() || two.error.is_some());
    owner.shutdown();
}
#[test]
fn blocked_output_does_not_block_exit_or_shutdown() {
    let owner = backend(2);
    let events = Arc::new(Events {
        blocked: true,
        ..Events::default()
    });
    owner
        .spawn(
            &command("printf output; exit 11"),
            size(),
            SessionLifetime::new(1, 5),
            ProcessLimits::default(),
            events.clone(),
        )
        .unwrap();
    events.wait(|s| s.exit.is_some());
    assert_eq!(
        events.state.lock().unwrap().exit,
        Some(ExitStatus::Code(11))
    );
    events.wait(|s| s.drain.is_some());
    assert_eq!(
        events.state.lock().unwrap().drain,
        Some(DrainOutcome::Truncated)
    );
    owner.shutdown();
}
#[test]
fn resize_and_quiet_reader_shutdown_are_interruptible() {
    let owner = backend(1);
    let events = Arc::new(Events::default());
    let session = owner
        .spawn(
            &command("printf ready; read x; stty size; sleep 60"),
            size(),
            SessionLifetime::new(1, 6),
            ProcessLimits::default(),
            events.clone(),
        )
        .unwrap();
    events.wait(|s| s.bytes.ends_with(b"ready"));
    assert_eq!(
        wait(session.resize(TerminalSize::new(93, 37).unwrap()).unwrap()),
        Ok(())
    );
    wait(session.write(b"go\n").unwrap());
    events.wait(|s| s.bytes.windows(5).any(|b| b == b"37 93"));
    let now = Instant::now();
    owner.shutdown();
    assert!(now.elapsed() < Duration::from_secs(2));
    assert!(events.state.lock().unwrap().exit.is_some());
    assert!(matches!(session.write(b"late"), Err(ProcessError::Closed)));
    assert!(matches!(
        owner.spawn(
            &command("exit 0"),
            size(),
            SessionLifetime::new(1, 7),
            ProcessLimits::default(),
            events.clone()
        ),
        Err(ProcessError::Closed)
    ));
}
#[test]
fn descendant_endpoint_has_bounded_drain_separate_from_exit() {
    let owner = backend(1);
    let events = Arc::new(Events::default());
    owner
        .spawn(
            &command("trap '' HUP; sleep 1 & printf final; exit 23"),
            size(),
            SessionLifetime::new(1, 8),
            ProcessLimits {
                drain_timeout: Duration::from_millis(40),
                ..ProcessLimits::default()
            },
            events.clone(),
        )
        .unwrap();
    events.wait(|s| s.exit.is_some() && s.drain.is_some());
    let state = events.state.lock().unwrap();
    assert_eq!(state.exit, Some(ExitStatus::Code(23)));
    assert_eq!(state.bytes, b"final");
    // The independent session leader preserves descendant output on both OSes.
    assert_eq!(state.drain, Some(DrainOutcome::Truncated));
    drop(state);
    owner.shutdown();
}
#[test]
fn failed_spawn_releases_admission_and_drop_reaps_child() {
    let owner = backend(1);
    let events = Arc::new(Events::default());
    let missing = CommandSpec::new(
        "/nonexistent-pty-fixture".into(),
        std::env::temp_dir(),
        vec![],
    )
    .unwrap();
    for i in 0..20 {
        assert!(matches!(
            owner.spawn(
                &missing,
                size(),
                SessionLifetime::new(1, i),
                ProcessLimits::default(),
                events.clone()
            ),
            Err(ProcessError::NotFound)
        ));
    }
    let session = owner
        .spawn(
            &command("sleep 60"),
            size(),
            SessionLifetime::new(1, 30),
            ProcessLimits::default(),
            events.clone(),
        )
        .unwrap();
    let pid = session.process_id();
    drop(owner);
    // SAFETY: waitpid with WNOHANG only inspects this child ID; the adapter must already have reaped it.
    assert_eq!(
        unsafe { libc::waitpid(pid as _, std::ptr::null_mut(), libc::WNOHANG) },
        -1
    );
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ECHILD)
    );
}
