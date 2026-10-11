//! Boundary contracts that do not change the test host's session or signals.
use crate::{
    cancellation::Cancellation,
    discovery::{Discovery, Observation},
    os,
    protocol::Kind,
};
use std::{
    os::fd::{AsRawFd, IntoRawFd},
    os::unix::net::UnixStream,
    time::{Duration, Instant},
};

#[test]
fn cancellation_without_a_live_workload_stays_idle_until_requested() {
    let now = Instant::now();
    let mut cancellation = Cancellation::new(1, 2, -1, 1, Duration::from_millis(40));
    assert!(cancellation.empty());
    assert_eq!(cancellation.delay(now), -1);
    assert!(!cancellation.take_term_applied());
    assert!(!cancellation.take_escalation_applied());
    let mut fds = Vec::new();
    cancellation.pollfds(&mut fds);
    assert!(fds.is_empty());
    cancellation.request(Kind::Terminate);
    // Queueing is not evidence of a signal or an escalation being applied.
    assert!(!cancellation.take_term_applied());
    assert!(!cancellation.take_escalation_applied());
    cancellation.stop();
    cancellation.request(Kind::Terminate);
    assert_eq!(cancellation.delay(now), -1);
    assert!(cancellation.empty());
}

#[test]
fn descriptor_adoption_sets_close_on_exec_and_retains_the_socket() {
    let (socket, peer) = UnixStream::pair().unwrap();
    let descriptor = socket.as_raw_fd();
    // Socket construction already sets CLOEXEC. Clear it so adoption must
    // establish the invariant instead of inheriting the expected result.
    // SAFETY: this test exclusively owns the live socket descriptor.
    assert_eq!(unsafe { libc::fcntl(descriptor, libc::F_SETFD, 0) }, 0);
    // SAFETY: read-only flag query of the same exclusively owned descriptor.
    assert_eq!(
        unsafe { libc::fcntl(descriptor, libc::F_GETFD) } & libc::FD_CLOEXEC,
        0
    );
    let adopted = os::owned(socket.into_raw_fd()).unwrap();
    assert_eq!(adopted.as_raw_fd(), descriptor);
    // SAFETY: adopted owns this descriptor throughout the read-only flag query.
    assert_ne!(
        unsafe { libc::fcntl(descriptor, libc::F_GETFD) } & libc::FD_CLOEXEC,
        0
    );
    drop(adopted);
    let mut buffer = [0];
    assert_eq!(std::io::Read::read(&mut &peer, &mut buffer).unwrap(), 0);
    assert_eq!(os::owned(-1).unwrap_err().raw_os_error(), Some(libc::EBADF));
}

#[test]
fn poll_reports_readability_without_consuming_the_socket_byte() {
    let (mut socket, mut peer) = UnixStream::pair().unwrap();
    let mut fds = [libc::pollfd {
        fd: socket.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    }];
    os::pause(&mut fds, 0).unwrap();
    assert_eq!(fds[0].revents, 0);
    std::io::Write::write_all(&mut peer, b"x").unwrap();
    os::pause(&mut fds, 1000).unwrap();
    assert_ne!(fds[0].revents & libc::POLLIN, 0);
    let mut byte = [0];
    std::io::Read::read_exact(&mut socket, &mut byte).unwrap();
    assert_eq!(&byte, b"x");
    os::pause(&mut [], 0).unwrap();
}

#[test]
fn live_inventory_reports_the_callers_real_session_and_group() {
    // SAFETY: read-only identity queries for this live test process.
    let (pid, sid, group) = unsafe { (libc::getpid(), libc::getsid(0), libc::getpgrp()) };
    let mut discovery = Discovery::new(sid).unwrap();
    let mut found = false;
    let deadline = Instant::now() + Duration::from_secs(5);
    while let Some(observation) = discovery.next() {
        assert!(
            Instant::now() < deadline,
            "process inventory did not terminate"
        );
        if let Observation::Member {
            pid: observed,
            group: observed_group,
        } = observation
        {
            if observed == pid {
                assert_eq!(observed_group, group);
                found = true;
            }
        }
    }
    assert!(found, "live caller absent from its own session inventory");
}
