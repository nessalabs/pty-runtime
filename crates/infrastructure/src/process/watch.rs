use super::error;
use pty_runtime_domain::process::ProcessError;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
/// Readiness descriptor, without process-global signal disposition changes.
pub(super) struct ExitWatch(OwnedFd);
impl ExitWatch {
    pub fn new(pid: u32) -> Result<Self, ProcessError> {
        #[cfg(target_os = "linux")]
        {
            // SAFETY: pidfd_open takes integer arguments and returns a new owned fd.
            let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) } as i32;
            if fd < 0 {
                return Err(error(std::io::Error::last_os_error()));
            }
            // SAFETY: successful syscall returned an exclusively owned descriptor.
            Ok(Self(unsafe { OwnedFd::from_raw_fd(fd) }))
        }
        #[cfg(target_os = "macos")]
        {
            // SAFETY: kqueue has no arguments and returns a new owned fd.
            let fd = unsafe { libc::kqueue() };
            if fd < 0 {
                return Err(error(std::io::Error::last_os_error()));
            }
            // SAFETY: successful kqueue returned an exclusively owned descriptor.
            let fd = unsafe { OwnedFd::from_raw_fd(fd) };
            // SAFETY: fd is live; fcntl only changes its close-on-exec flag.
            if unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
                return Err(error(std::io::Error::last_os_error()));
            }
            let event = libc::kevent {
                ident: pid as _,
                filter: libc::EVFILT_PROC,
                flags: libc::EV_ADD | libc::EV_ENABLE | libc::EV_ONESHOT,
                fflags: libc::NOTE_EXIT,
                data: 0,
                udata: std::ptr::null_mut(),
            };
            // SAFETY: event is initialized; no output list; null timeout is valid.
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
                return Err(error(std::io::Error::last_os_error()));
            }
            Ok(Self(fd))
        }
    }
    pub fn fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}
