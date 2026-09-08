//! Syscall ownership for a fresh, single-threaded helper process.
use std::{
    io,
    os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
};

pub fn owned(fd: RawFd) -> io::Result<OwnedFd> {
    // SAFETY: the caller hands this fresh process an exclusively owned descriptor.
    if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful fcntl verified the caller-transferred descriptor is live.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}
pub fn close_unrelated(keep: &[RawFd]) -> io::Result<()> {
    // This executes only after a fresh exec or a fork of our single-threaded
    // helper. No other thread can open/reuse a descriptor during this inventory.
    let mut descriptors = Vec::new();
    for entry in std::fs::read_dir("/dev/fd")? {
        let name = entry?.file_name();
        if let Some(fd) = name.to_str().and_then(|name| name.parse::<RawFd>().ok()) {
            if fd > 2 && !keep.contains(&fd) {
                descriptors.push(fd);
            }
        }
        if descriptors.len() > 65536 {
            return Err(io::Error::new(
                io::ErrorKind::OutOfMemory,
                "descriptor inventory",
            ));
        }
    }
    for fd in descriptors {
        // SAFETY: inventory is private to this single-threaded process. The
        // directory iterator has closed its own fd; closing it again is harmless
        // because no descriptor-opening operation occurs between these steps.
        unsafe {
            libc::close(fd);
        }
    }
    Ok(())
}
pub fn ignore_supervision_signals() -> io::Result<()> {
    for signal in [
        libc::SIGHUP,
        libc::SIGTERM,
        libc::SIGINT,
        libc::SIGQUIT,
        libc::SIGTTOU,
        libc::SIGTTIN,
        libc::SIGTSTP,
        libc::SIGPIPE,
    ] {
        // SAFETY: this is the fresh helper's private signal disposition; it is
        // single threaded and installs no callbacks or shared host state.
        if unsafe { libc::signal(signal, libc::SIG_IGN) } == libc::SIG_ERR {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}
pub fn session(slave: RawFd) -> io::Result<libc::pid_t> {
    // SAFETY: this process is the newly executed helper, never a host thread;
    // slave is its exclusively inherited child PTY endpoint.
    let sid = unsafe { libc::setsid() };
    if sid < 0 || unsafe { libc::ioctl(slave, libc::TIOCSCTTY as _, 0) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(sid)
}
pub fn own_group() -> io::Result<()> {
    // SAFETY: changes only this fresh helper's group inside its owned session.
    if unsafe { libc::setpgid(0, 0) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
pub fn fork() -> io::Result<libc::pid_t> {
    // SAFETY: every caller is our fresh single-threaded helper. No host locks,
    // host callbacks, or other threads exist in this process at fork.
    let pid = unsafe { libc::fork() };
    if pid < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(pid)
}
pub fn wait(pid: libc::pid_t) -> io::Result<Option<i32>> {
    let mut status = 0;
    // SAFETY: callers retain sole reaping ownership of this direct child. The
    // status pointer is initialized and writable for the duration of waitpid.
    match unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) } {
        0 => Ok(None),
        result if result == pid => Ok(Some(status)),
        _ => Err(io::Error::last_os_error()),
    }
}
pub fn pause(fds: &mut [libc::pollfd], milliseconds: i32) -> io::Result<()> {
    // SAFETY: each descriptor remains owned by the event loop while poll runs;
    // the fully initialized array is exclusively borrowed for its whole length.
    if unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as _, milliseconds) } < 0 {
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
    Ok(())
}
pub fn terminate_own_group() -> ! {
    // SAFETY: no numeric group lookup: the caller signals its actual current
    // group. Its own membership pins that group until SIGKILL terminates it.
    unsafe {
        libc::kill(0, libc::SIGKILL);
        libc::_exit(125);
    }
}
pub struct ExitWatch(OwnedFd);
impl ExitWatch {
    pub fn new(pid: libc::pid_t) -> io::Result<Self> {
        #[cfg(target_os = "linux")]
        {
            // SAFETY: the retained direct child PID cannot be reused before
            // its exclusive owner reaps it; the syscall creates an owned handle.
            let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) } as i32;
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            owned(fd).map(Self)
        }
        #[cfg(target_os = "macos")]
        {
            // SAFETY: kqueue has no input pointers and creates an owned handle.
            let fd = unsafe { libc::kqueue() };
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            let fd = owned(fd)?;
            let event = libc::kevent {
                ident: pid as _,
                filter: libc::EVFILT_PROC,
                flags: libc::EV_ADD | libc::EV_ENABLE | libc::EV_ONESHOT,
                fflags: libc::NOTE_EXIT,
                data: 0,
                udata: std::ptr::null_mut(),
            };
            // SAFETY: the initialized event references a retained child identity;
            // fd is live and no output events or timeout pointers are supplied.
            if unsafe {
                libc::kevent(
                    fd.as_raw_fd(),
                    &event,
                    1,
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null(),
                )
            } < 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(Self(fd))
        }
    }
    pub fn fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}
