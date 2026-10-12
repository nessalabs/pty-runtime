//! Recovery contracts executed by the single-threaded Cargo test entry point.
use crate::*;
use cleanup::Progress;
use fault_fixture::{Session, finish_sweep, reap_remaining_children};
use fault_injection::{Failure, Operation};
use protocol::{Channel, Kind};
use std::{
    os::{fd::AsRawFd, unix::net::UnixStream},
    time::{Duration, Instant},
};

fn discovery_failure_retains_workload_until_retry(session: &Session) {
    let mut workload = session.launch();
    let mut sweep = session.sweep();
    let now = Instant::now();
    let failure = Failure::arm(Operation::Discovery, 1);
    assert!(matches!(sweep.step(now), Progress::Retained));
    assert_eq!(failure.remaining(), 0);
    drop(failure);
    assert_eq!(sweep.idle_delay(now), 50);
    assert!(matches!(
        sweep.step(now + Duration::from_millis(49)),
        Progress::Working
    ));
    assert!(workload.0.reap().unwrap().is_none());
    finish_sweep(&mut sweep, &mut workload, now + Duration::from_millis(50));
}

fn anchor_failure_retains_workload_until_retry(session: &Session) {
    let mut workload = session.launch();
    let mut sweep = session.sweep();
    let now = Instant::now();
    let deadline = now + Duration::from_secs(5);
    let failure = Failure::arm(Operation::Anchor, 1);
    loop {
        assert!(Instant::now() < deadline, "failed anchor was not retried");
        match sweep.step(now) {
            Progress::Retained => break,
            Progress::Working => {}
            Progress::ReadyToRetire(_) => panic!("live workload was forgotten"),
        }
    }
    assert_eq!(failure.remaining(), 0);
    drop(failure);
    assert!(workload.0.reap().unwrap().is_none());
    assert_eq!(sweep.idle_delay(now), 50);
    finish_sweep(&mut sweep, &mut workload, now + Duration::from_millis(50));
}

fn failed_handoff_preserves_channels_and_cleanup_ownership(session: &Session) {
    let mut workload = session.launch();
    let mut sweep = session.sweep();
    let (owner, owner_peer) = UnixStream::pair().unwrap();
    let (peer, peer_peer) = UnixStream::pair().unwrap();
    let mut links = links::Links::new(owner, peer, 1).unwrap();
    let mut owner_peer = Channel::new(owner_peer, 1).unwrap();
    let mut peer_peer = Channel::new(peer_peer, 1).unwrap();
    let failure = Failure::arm(Operation::Handoff, 1);
    let error =
        successor::handoff(&mut links, &session.host, session.sid, session.sid, 1).unwrap_err();
    assert_eq!(error.raw_os_error(), Some(libc::EAGAIN));
    assert_eq!(failure.remaining(), 0);
    drop(failure);
    let now = Instant::now();
    sweep.retain_for_retry(now);
    assert_eq!(sweep.idle_delay(now), 50);
    assert!(matches!(sweep.step(now), Progress::Working));
    assert!(workload.0.reap().unwrap().is_none());
    links.fault();
    links.flush();
    for channel in [&mut owner_peer, &mut peer_peer] {
        let message = channel.receive().unwrap().expect("fault status survives");
        assert_eq!(message.kind, Kind::Fault);
        assert_eq!(message.values, [0; 4]);
        assert!(!channel.eof());
    }
    assert!(links.owner.healthy() && links.peer.healthy());
    finish_sweep(&mut sweep, &mut workload, now + Duration::from_millis(50));
}

fn missing_anchors_preserve_grace_and_escalate_actual_workload(session: &Session) {
    let mut workload = session.launch();
    let grace = Duration::from_millis(250);
    let mut cancellation = cancellation::Cancellation::new(
        session.sid,
        session.sid,
        session.host.as_raw_fd(),
        1,
        grace,
    );
    let failure = Failure::arm(Operation::Anchor, 2);
    let now = Instant::now();
    cancellation.request(Kind::Terminate);
    assert!(cancellation.tick(&workload.0, now, false));
    assert_eq!(failure.remaining(), 0);
    drop(failure);
    assert!(cancellation.empty());
    assert!(!cancellation.take_term_applied());
    assert!(!cancellation.take_escalation_applied());
    assert_eq!(cancellation.delay(now), 250);
    assert_eq!(cancellation.delay(now + Duration::from_millis(249)), 1);
    assert!(workload.0.reap().unwrap().is_none());
    cancellation.tick(&workload.0, now + grace, false);
    assert!(cancellation.take_escalation_applied());
    assert!(!cancellation.take_escalation_applied());
    assert!(!cancellation.take_term_applied());
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut status = None;
    loop {
        assert!(
            Instant::now() < deadline,
            "escalation did not reap its children"
        );
        status = status.or(workload.0.reap().unwrap());
        cancellation.tick(&workload.0, now + grace, true);
        if cancellation.empty() && status.is_some() {
            break;
        }
        let mut descriptors = Vec::new();
        cancellation.pollfds(&mut descriptors);
        os::pause(&mut descriptors, 10).unwrap();
    }
    fault_fixture::assert_killed(status.unwrap());
    assert_eq!(cancellation.delay(now + grace), -1);
}

pub(super) fn run() {
    let session = Session::new();
    for (name, scenario) in [
        (
            "discovery retry",
            discovery_failure_retains_workload_until_retry as fn(&Session),
        ),
        ("anchor retry", anchor_failure_retains_workload_until_retry),
        (
            "handoff failure",
            failed_handoff_preserves_channels_and_cleanup_ownership,
        ),
        (
            "unavailable anchor grace",
            missing_anchors_preserve_grace_and_escalate_actual_workload,
        ),
    ] {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| scenario(&session)));
        reap_remaining_children();
        if let Err(panic) = result {
            std::panic::resume_unwind(panic);
        }
        println!("passed: {name}");
    }
}
