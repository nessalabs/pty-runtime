//! Real PTY and child ownership for the single-threaded fault test executable.
use crate::{
    cleanup::{Progress, Sweep},
    os,
    workload::Workload,
};
use std::{
    ffi::OsString,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    time::{Duration, Instant},
};

pub struct Session {
    pub host: OwnedFd,
    slave: OwnedFd,
    pub sid: i32,
}

impl Session {
    pub fn new() -> Self {
        let mut host = -1;
        let mut slave = -1;
        // SAFETY: initialized output descriptors; optional name/termios/winsize
        // pointers are null. This executable has no other threads.
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut host,
                    &mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            },
            0
        );
        // SAFETY: openpty transferred two unique descriptors on success.
        let (host, slave) = unsafe { (OwnedFd::from_raw_fd(host), OwnedFd::from_raw_fd(slave)) };
        // SAFETY: read-only identity of the parent runner's inherited group.
        let inherited_group = unsafe { libc::getpgrp() };
        let sid = os::session(slave.as_raw_fd()).unwrap();
        os::ignore_supervision_signals().unwrap();
        // SAFETY: read-only identity checks after creating our private session.
        assert_eq!(sid, unsafe { libc::getpid() });
        assert_eq!(sid, unsafe { libc::getpgrp() });
        assert_ne!(sid, inherited_group);
        Self { host, slave, sid }
    }

    pub fn launch(&self) -> LiveWorkload {
        let arguments: Vec<OsString> = [
            "/bin/sh",
            "-c",
            "trap '' TERM HUP; printf 'ready\\n'; while :; do read value; done",
        ]
        .into_iter()
        .map(OsString::from)
        .collect();
        let workload =
            LiveWorkload(Workload::launch(&arguments, self.slave.try_clone().unwrap()).unwrap());
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut bytes = Vec::new();
        while !bytes.windows(5).any(|window| window == b"ready") {
            assert!(Instant::now() < deadline, "workload did not become ready");
            let mut descriptors = [libc::pollfd {
                fd: self.host.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            }];
            os::pause(&mut descriptors, 10).unwrap();
            if descriptors[0].revents & libc::POLLIN != 0 {
                let mut buffer = [0; 128];
                // SAFETY: the PTY remains owned and the buffer is writable.
                let count = unsafe {
                    libc::read(
                        self.host.as_raw_fd(),
                        buffer.as_mut_ptr().cast(),
                        buffer.len(),
                    )
                };
                assert!(count > 0, "ready stream closed");
                bytes.extend_from_slice(&buffer[..count as usize]);
            }
        }
        // SAFETY: the actual child remains owned and unreaped throughout these queries.
        assert_eq!(unsafe { libc::getsid(workload.0.pid()) }, self.sid);
        assert_eq!(unsafe { libc::getpgid(workload.0.pid()) }, workload.0.pid());
        assert_eq!(
            unsafe { libc::tcgetpgrp(self.host.as_raw_fd()) },
            workload.0.pid()
        );
        assert_ne!(workload.0.pid(), self.sid);
        workload
    }

    pub fn sweep(&self) -> Sweep {
        Sweep::new(self.sid, self.sid, None, self.host.as_raw_fd(), 1)
    }
}

pub struct LiveWorkload(pub Workload);

impl Drop for LiveWorkload {
    fn drop(&mut self) {
        let _ = self.0.signal(libc::SIGKILL);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.0.settled() && Instant::now() < deadline {
            let _ = self.0.reap();
            if !self.0.settled() {
                let mut descriptors: Vec<_> = self.0.pollfd().into_iter().collect();
                let _ = os::pause(&mut descriptors, 10);
            }
        }
        assert!(
            self.0.settled(),
            "owned workload did not settle during cleanup"
        );
    }
}

pub fn assert_killed(status: i32) {
    assert!(libc::WIFSIGNALED(status));
    assert_eq!(libc::WTERMSIG(status), libc::SIGKILL);
}

pub fn finish_sweep(sweep: &mut Sweep, workload: &mut LiveWorkload, earliest: Instant) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut status = None;
    loop {
        assert!(Instant::now() < deadline, "cleanup retry did not finish");
        status = status.or(workload.0.reap().unwrap());
        if let Progress::ReadyToRetire(members) = sweep.step(Instant::now().max(earliest)) {
            assert!(!members.own && !members.peer);
            if let Some(status) = status {
                assert_killed(status);
                return;
            }
        }
        let mut descriptors = Vec::new();
        sweep.pollfds(&mut descriptors);
        os::pause(&mut descriptors, 10).unwrap();
    }
}

pub fn reap_remaining_children() {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let mut status = 0;
        // SAFETY: this single-threaded test process exclusively owns every child
        // it creates; no library runner or unrelated child exists here.
        let pid = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) };
        if pid < 0 {
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(libc::ECHILD)
            );
            return;
        }
        assert!(
            Instant::now() < deadline,
            "anchor ownership leaked after cleanup"
        );
        if pid == 0 {
            os::pause(&mut [], 10).unwrap();
        }
    }
}
