//! Internal injection barriers prove blocking launch work cannot stall supervision.
// This file is included as a unit-test module; integration compilation is disabled
// with the feature-independent include flag below.
#![cfg(test)]
use super::{backend::UnixProcessBackend, spawner};
#[path = "process_events.rs"]
mod support;
use pty_runtime_application::process::IProcessBackend;
use pty_runtime_domain::{
    SessionLifetime,
    process::{ProcessError, ProcessLimits},
    terminal::TerminalSize,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};
use support::*;
fn blocked_owner(
    fail: bool,
) -> (
    Arc<UnixProcessBackend>,
    mpsc::Receiver<()>,
    mpsc::Sender<()>,
    mpsc::Receiver<u32>,
) {
    let count = AtomicUsize::new(0);
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let release_rx = Mutex::new(release_rx);
    let (pid_tx, pid_rx) = mpsc::channel();
    let after_launch = Arc::new(move |pid| {
        pid_tx.send(pid).unwrap();
    });
    let hook = Arc::new(move || {
        if count.fetch_add(1, Ordering::SeqCst) == 1 {
            entered_tx.send(()).unwrap();
            release_rx
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            if fail {
                return Err(ProcessError::PermissionDenied);
            }
        }
        Ok(())
    });
    (
        Arc::new(
            UnixProcessBackend::build(
                vec![std::env::temp_dir()],
                3,
                spawner::Options {
                    hook: Some(hook),
                    after_launch: Some(after_launch),
                },
            )
            .unwrap(),
        ),
        entered_rx,
        release_tx,
        pid_rx,
    )
}
#[test]
fn blocked_and_failed_spawn_does_not_delay_existing_controls_or_reaping() {
    let (owner, entered, release, _pids) = blocked_owner(true);
    let events = Arc::new(Events::default());
    let session = owner
        .spawn(
            &command("printf ready; read x; printf acknowledged; sleep 60"),
            size(),
            SessionLifetime::new(81, 1),
            ProcessLimits::default(),
            events.clone(),
        )
        .unwrap();
    events.wait(|s| s.bytes == b"ready");
    let blocked_owner = owner.clone();
    let blocked = std::thread::spawn(move || {
        blocked_owner.spawn(
            &command("sleep 60"),
            size(),
            SessionLifetime::new(81, 2),
            ProcessLimits::default(),
            Arc::new(Events::default()),
        )
    });
    entered.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(
        wait(session.resize(TerminalSize::new(90, 30).unwrap()).unwrap()),
        Ok(())
    );
    assert_eq!(wait(session.write(b"go\n").unwrap()).written, 3);
    events.wait(|s| s.bytes.ends_with(b"acknowledged"));
    session.request_cancel().unwrap();
    events.wait(|s| s.exit.is_some());
    release.send(()).unwrap();
    assert!(matches!(
        blocked.join().unwrap(),
        Err(ProcessError::PermissionDenied)
    ));
    owner.shutdown();
}
#[test]
fn immediate_shutdown_reaps_existing_child_before_stalled_launch_finishes() {
    let (owner, entered, release, pids) = blocked_owner(false);
    let events = Arc::new(Events::default());
    let limits = ProcessLimits {
        terminate_grace: Duration::from_secs(86400),
        ..ProcessLimits::default()
    };
    owner
        .spawn(
            &command("trap '' TERM; printf ready; sleep 60"),
            size(),
            SessionLifetime::new(82, 1),
            limits,
            events.clone(),
        )
        .unwrap();
    events.wait(|s| s.bytes == b"ready");
    let blocked_owner = owner.clone();
    let blocked = std::thread::spawn(move || {
        blocked_owner.spawn(
            &command("sleep 60"),
            size(),
            SessionLifetime::new(82, 2),
            ProcessLimits::default(),
            Arc::new(Events::default()),
        )
    });
    entered.recv_timeout(Duration::from_secs(2)).unwrap();
    let stop_owner = owner.clone();
    let stopping = std::thread::spawn(move || stop_owner.shutdown_now());
    let before = Instant::now();
    events.wait(|s| s.exit.is_some());
    assert!(before.elapsed() < Duration::from_secs(2));
    release.send(()).unwrap();
    assert!(matches!(blocked.join().unwrap(), Err(ProcessError::Closed)));
    stopping.join().unwrap();
    let _first = pids.recv_timeout(Duration::from_secs(2)).unwrap();
    let late_pid = pids.recv_timeout(Duration::from_secs(2)).unwrap();
    // SAFETY: late_pid belongs to the fixture's actual late-created child;
    // shutdown must have already collected it before returning.
    assert_eq!(
        unsafe { libc::waitpid(late_pid as _, std::ptr::null_mut(), libc::WNOHANG) },
        -1
    );
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ECHILD)
    );
}
