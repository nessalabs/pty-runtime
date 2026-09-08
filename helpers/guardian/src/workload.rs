//! Only this direct-child owner can produce the actual workload exit record.
use crate::os;
use std::{
    ffi::OsString,
    fs::File,
    io,
    os::{
        fd::OwnedFd,
        unix::process::{CommandExt, ExitStatusExt},
    },
    process::{Child, Command, Stdio},
};
pub struct Workload {
    child: Child,
    watch: Option<os::ExitWatch>,
    reaped: bool,
    lost: bool,
}
impl Workload {
    pub fn launch(arguments: &[OsString], slave: OwnedFd) -> io::Result<Self> {
        let (executable, arguments) = arguments.split_first().ok_or(io::ErrorKind::InvalidInput)?;
        let slave = File::from(slave);
        let mut command = Command::new(executable);
        command
            .args(arguments)
            .stdin(Stdio::from(slave.try_clone()?))
            .stdout(Stdio::from(slave.try_clone()?))
            .stderr(Stdio::from(slave));
        // SAFETY: stdio mapping precedes this closure. It uses only signal,
        // setpgid, getpid and tcsetpgrp in our freshly forked workload child.
        // All helper endpoints have CLOEXEC and close when actual exec succeeds.
        unsafe {
            command.pre_exec(|| {
                if libc::setpgid(0, 0) < 0 || libc::tcsetpgrp(0, libc::getpid()) < 0 {
                    return Err(io::Error::last_os_error());
                }
                for signal in [
                    libc::SIGHUP,
                    libc::SIGTERM,
                    libc::SIGINT,
                    libc::SIGQUIT,
                    libc::SIGTTOU,
                    libc::SIGTTIN,
                    libc::SIGTSTP,
                    libc::SIGPIPE,
                    libc::SIGCHLD,
                ] {
                    if libc::signal(signal, libc::SIG_DFL) == libc::SIG_ERR {
                        return Err(io::Error::last_os_error());
                    }
                }
                Ok(())
            });
        }
        // Command::spawn's CLOEXEC error pipe distinguishes exec success from
        // helper readiness and reaps a child whose pre_exec/exec fails.
        let child = command.spawn()?;
        let watch = os::ExitWatch::new(child.id() as i32).ok();
        Ok(Self {
            child,
            watch,
            reaped: false,
            lost: false,
        })
    }
    pub fn pid(&self) -> libc::pid_t {
        self.child.id() as libc::pid_t
    }
    pub fn reap(&mut self) -> io::Result<Option<i32>> {
        if self.reaped || self.lost {
            return Ok(None);
        }
        let status = match self.child.try_wait() {
            Ok(status) => status,
            Err(error) => {
                self.lost = true;
                self.watch = None;
                return Err(error);
            }
        };
        match status {
            Some(status) => {
                self.reaped = true;
                self.watch = None;
                Ok(Some(status.into_raw()))
            }
            None => Ok(None),
        }
    }
    pub fn reaped(&self) -> bool {
        self.reaped
    }
    pub fn settled(&self) -> bool {
        self.reaped || self.lost
    }
    pub fn signal(&self, signal: i32) -> io::Result<bool> {
        if self.reaped || self.lost {
            return Ok(false);
        }
        // SAFETY: this is the actual unreaped direct child, exclusively owned
        // by this helper. Its PID cannot be reused before our reap. This does
        // not authorize any numeric process-group lookup or post-reap fallback.
        if unsafe { libc::kill(self.pid(), signal) } < 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ESRCH) {
                return Err(error);
            }
            return Ok(false);
        }
        Ok(true)
    }
    pub fn pollfd(&self) -> Option<libc::pollfd> {
        self.watch.as_ref().map(|watch| libc::pollfd {
            fd: watch.fd(),
            events: libc::POLLIN,
            revents: 0,
        })
    }
    pub fn needs_poll(&self) -> bool {
        !self.settled() && self.watch.is_none()
    }
}
