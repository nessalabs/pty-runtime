//! Helper damage must clean the victim without disabling another admitted PTY.
#[path = "fixtures/process_support.rs"]
mod support;
use pty_runtime_application::process::IProcessBackend;
use pty_runtime_domain::{
    SessionLifetime,
    process::{DrainOutcome, ProcessError, ProcessLimits},
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[test]
fn guardian_loss_reclaims_known_descendants_and_preserves_another_session() {
    damaged_session(false);
}

#[test]
fn sentinel_loss_reclaims_known_descendants_and_preserves_another_session() {
    damaged_session(true);
}

fn damaged_session(sentinel: bool) {
    let owner = support::backend(2);
    let survivor_events = Arc::new(support::Events::default());
    let survivor = owner
        .spawn(
            &support::command(
                "printf 'ready\\n'; while IFS= read -r line; do printf 'ack:%s\\n' \"$line\"; done",
            ),
            support::size(),
            SessionLifetime::new(801, 1),
            ProcessLimits::default(),
            survivor_events.clone(),
        )
        .unwrap();
    survivor_events.wait(|s| s.bytes.windows(5).any(|b| b == b"ready"));
    let victim_events = Arc::new(support::Events::default());
    let victim = owner
        .spawn(
            &support::command(
                "trap '' HUP TERM; sleep 300 & printf '%s %s %s\\n' \"$$\" \"$PPID\" \"$!\"; wait",
            ),
            support::size(),
            SessionLifetime::new(801, 2),
            ProcessLimits::default(),
            victim_events.clone(),
        )
        .unwrap();
    victim_events.wait(|s| s.bytes.contains(&b'\n'));
    let bytes = victim_events.state.lock().unwrap().bytes.clone();
    let ids: Vec<i32> = std::str::from_utf8(&bytes)
        .unwrap()
        .split_whitespace()
        .map(|value| value.parse().unwrap())
        .collect();
    assert_eq!(ids.len(), 3);
    assert_eq!(victim.process_id(), ids[0] as u32);
    // SAFETY: the fixture has acknowledged these live processes; only its owned
    // helper is damaged. No signal is sent to a discovered unrelated process.
    let sid = unsafe { libc::getsid(ids[0]) };
    assert!(sid > 0);
    assert_ne!(unsafe { libc::getsid(survivor.process_id() as i32) }, sid);
    let helper = if sentinel { sid } else { ids[1] };
    assert_eq!(unsafe { libc::kill(helper, libc::SIGKILL) }, 0);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let state = victim_events.state.lock().unwrap();
        if state.failure.is_some() && state.drain.is_some() {
            break;
        }
        drop(state);
        assert!(
            Instant::now() < deadline,
            "damaged session did not finish drain/failure"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let state = victim_events.state.lock().unwrap();
    assert_eq!(state.failure, Some(ProcessError::Internal));
    // Supervision failure may end the bounded drain window. It must retain
    // the explicit truncation fact rather than manufacture workload completion.
    assert!(matches!(
        state.drain,
        Some(DrainOutcome::Eof | DrainOutcome::Truncated)
    ));
    if !sentinel {
        assert_eq!(state.exit, None);
    }
    drop(state);
    for pid in [ids[0], ids[2]] {
        while live(pid) {
            assert!(
                Instant::now() < deadline,
                "known victim process {pid} survived recovery"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    let written = support::wait(survivor.write(b"after-helper-loss\n").unwrap());
    assert_eq!(written.written, 18);
    assert_eq!(written.error, None);
    let acknowledgement = b"ack:after-helper-loss";
    survivor_events.wait(|s| {
        s.bytes
            .windows(acknowledgement.len())
            .any(|b| b == acknowledgement)
    });
    assert_eq!(survivor_events.state.lock().unwrap().exit, None);
    support::wait(survivor.resize(support::size()).unwrap()).unwrap();
    owner.shutdown();
}

fn live(pid: i32) -> bool {
    let output = std::process::Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    assert!(
        matches!(output.status.code(), Some(0 | 1)) && output.stderr.is_empty(),
        "process inventory failed: {:?}",
        output.status
    );
    output.stdout.iter().any(u8::is_ascii_alphabetic)
        && !output.stdout.contains(&b'Z')
        && !output.stdout.contains(&b'E')
}
