//! Independent packaged-adapter cancellation against changing foreground ownership.
#[path = "fixtures/process_support.rs"]
mod support;
use pty_runtime_application::{
    diagnostics::{LatencyKind, RuntimeDiagnostics},
    process::{IProcessBackend, IProcessEvents, OutputAcceptance},
};
use pty_runtime_domain::{
    SessionLifetime,
    process::{
        CommandSpec, DrainOutcome, EnvironmentPolicy, ExitStatus, ProcessError, ProcessLimits,
    },
};
use std::{
    io::Write,
    os::unix::process::CommandExt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

static TERM: AtomicBool = AtomicBool::new(false);
extern "C" fn requested(_: libc::c_int) {
    TERM.store(true, Ordering::Release);
}

#[test]
fn foreground_switch_fixture() {
    if std::env::var_os("PTY_FOREGROUND_SWITCH_FIXTURE").is_none() {
        return;
    }
    // This is a self-executed fixture. Child branches after fork use only
    // async-signal-safe libc calls and never return to the Rust test harness.
    unsafe {
        libc::signal(libc::SIGHUP, libc::SIG_IGN);
        libc::signal(libc::SIGTTOU, libc::SIG_IGN);
        libc::signal(libc::SIGTERM, requested as *const () as libc::sighandler_t);
    }
    let first = member();
    let second = member();
    assert_eq!(unsafe { libc::tcsetpgrp(0, first) }, 0);
    println!("\nSWITCH_READY {first} {second}");
    std::io::stdout().flush().unwrap();
    while !TERM.load(Ordering::Acquire) {
        std::thread::sleep(Duration::from_millis(1));
    }
    if std::env::var("PTY_FOREGROUND_SWITCH_FIXTURE").unwrap() != "stable" {
        assert_eq!(unsafe { libc::tcsetpgrp(0, second) }, 0);
        println!("SWITCHED");
    } else {
        println!("FOREGROUND_STABLE");
    }
    std::io::stdout().flush().unwrap();
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn member() -> i32 {
    let mut pipe = [-1; 2];
    // SAFETY: pipe receives two owned descriptors. The fork child does no Rust
    // allocation, locking or destruction before its terminal _exit/pause loop.
    unsafe {
        assert_eq!(libc::pipe(pipe.as_mut_ptr()), 0);
        let pid = libc::fork();
        assert!(pid >= 0);
        if pid == 0 {
            libc::close(pipe[0]);
            if libc::setpgid(0, 0) < 0 {
                libc::_exit(119);
            }
            libc::signal(libc::SIGTERM, libc::SIG_IGN);
            libc::signal(libc::SIGHUP, libc::SIG_IGN);
            let byte = [1_u8];
            libc::write(pipe[1], byte.as_ptr().cast(), 1);
            libc::close(pipe[1]);
            loop {
                libc::pause();
            }
        }
        libc::close(pipe[1]);
        let mut byte = [0_u8];
        assert_eq!(libc::read(pipe[0], byte.as_mut_ptr().cast(), 1), 1);
        libc::close(pipe[0]);
        pid
    }
}

#[test]
fn cancellation_cleans_both_foreground_generations_and_preserves_outside_session() {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        switch_and_cancel(false);
        sender.send(()).unwrap();
    });
    assert_eq!(receiver.recv_timeout(Duration::from_secs(8)), Ok(()));
}

fn switch_and_cancel(stable: bool) {
    let mut command = std::process::Command::new("/bin/sleep");
    command.arg("10");
    // SAFETY: only setsid runs between this child fork and exec.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
    let mut outside = command.spawn().unwrap();
    let owner = support::backend(1);
    let events = Arc::new(support::Events::default());
    let spec = CommandSpec::new(
        std::env::current_exe().unwrap(),
        std::env::temp_dir(),
        vec![
            "--exact".into(),
            "foreground_switch_fixture".into(),
            "--nocapture".into(),
        ],
    )
    .unwrap()
    .with_environment(
        EnvironmentPolicy::Empty,
        vec![],
        vec![(
            "PTY_FOREGROUND_SWITCH_FIXTURE".into(),
            if stable { "stable" } else { "1" }.into(),
        )],
    )
    .unwrap();
    let diagnostics = RuntimeDiagnostics::new();
    let measured = Arc::new(MeasuredEvents {
        events: events.clone(),
        diagnostics: diagnostics.clone(),
    });
    let session = owner
        .spawn(
            &spec,
            support::size(),
            SessionLifetime::new(702, 1),
            ProcessLimits {
                terminate_grace: Duration::from_millis(200),
                ..Default::default()
            },
            measured,
        )
        .unwrap();
    events.wait(|state| {
        state.bytes.ends_with(b"\n")
            && state
                .bytes
                .windows(13)
                .any(|bytes| bytes == b"SWITCH_READY ")
    });
    let bytes = events.state.lock().unwrap().bytes.clone();
    let line = std::str::from_utf8(&bytes)
        .unwrap()
        .lines()
        .find(|line| line.starts_with("SWITCH_READY "))
        .unwrap();
    let members: Vec<i32> = line
        .split_whitespace()
        .skip(1)
        .map(|pid| pid.parse().unwrap())
        .collect();
    assert_eq!(members.len(), 2);
    let sid = unsafe { libc::getsid(session.process_id() as i32) };
    assert!(
        members
            .iter()
            .all(|pid| unsafe { libc::getsid(*pid) } == sid)
    );
    if stable {
        assert_ne!(members[0], session.process_id() as i32);
        assert_eq!(unsafe { libc::getpgid(members[0]) }, members[0]);
        assert_eq!(
            unsafe { libc::getpgid(session.process_id() as i32) },
            session.process_id() as i32
        );
    }
    session.request_cancel().unwrap();
    events.wait(|state| state.exit.is_some() && state.drain.is_some());
    owner.shutdown();
    let state = events.state.lock().unwrap();
    if stable {
        assert!(
            state
                .bytes
                .windows(17)
                .any(|bytes| bytes == b"FOREGROUND_STABLE")
        );
        let dispatch = diagnostics.snapshot(LatencyKind::CancelDispatch);
        assert_eq!(dispatch.samples(), 1);
        assert_eq!(dispatch.failures, 0);
        assert_eq!(dispatch.unavailable, 0);
    } else {
        assert!(state.bytes.windows(8).any(|bytes| bytes == b"SWITCHED"));
    }
    assert_eq!(state.exit, Some(ExitStatus::Signal(libc::SIGKILL)));
    assert_eq!(state.failure, None);
    drop(state);
    let deadline = Instant::now() + Duration::from_secs(1);
    while members.iter().any(|pid| live(*pid)) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    let leaked = members.iter().any(|pid| live(*pid));
    let outside_survived = outside.try_wait().unwrap().is_none();
    let _ = outside.kill();
    outside.wait().unwrap();
    assert!(
        !leaked,
        "a former/current foreground workload survived cleanup"
    );
    assert!(
        outside_survived,
        "outside-session control process was affected"
    );
}

fn live(pid: i32) -> bool {
    let output = std::process::Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    output.status.success()
        && output.stdout.iter().any(|byte| byte.is_ascii_alphabetic())
        && !output.stdout.contains(&b'Z')
        && !output.stdout.contains(&b'E')
}

#[test]
fn dispatch_ack_requires_a_distinct_verified_foreground_group() {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        switch_and_cancel(true);
        sender.send(()).unwrap();
    });
    assert_eq!(receiver.recv_timeout(Duration::from_secs(8)), Ok(()));
}

struct MeasuredEvents {
    events: Arc<support::Events>,
    diagnostics: Arc<RuntimeDiagnostics>,
}
impl IProcessEvents for MeasuredEvents {
    fn diagnostics(&self) -> Option<Arc<RuntimeDiagnostics>> {
        Some(self.diagnostics.clone())
    }
    fn output(&self, bytes: &[u8]) -> OutputAcceptance {
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
