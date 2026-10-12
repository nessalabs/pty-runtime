//! Stack-only syscall failures copied before fork. No TLS, locks, allocation or
//! callbacks are accessed in the child; consumption is reported over a real fd.
use super::write_inner;
use pty_runtime_domain::process::ProcessError;
use std::{
    io::Read,
    os::{fd::AsRawFd, unix::net::UnixStream},
    path::Path,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Point {
    SigFill,
    InstallMask,
    RestoreMask,
    Fork,
    Open,
    Write,
    Chmod,
    Close,
    Wait,
}

#[derive(Clone, Copy)]
pub(super) struct Fault {
    point: Point,
    result: i32,
    errno: i32,
    remaining: u8,
    report: i32,
}
impl Fault {
    pub(super) fn take(&mut self, point: Point) -> Option<i32> {
        if self.point != point || self.remaining == 0 {
            return None;
        }
        self.remaining -= 1;
        // SAFETY: report is a retained inherited socket. One scalar byte is
        // written without Rust callbacks; errno is this thread's private slot.
        unsafe {
            let _ = libc::write(self.report, b"F".as_ptr().cast(), 1);
            #[cfg(target_os = "macos")]
            {
                *libc::__error() = self.errno;
            }
            #[cfg(target_os = "linux")]
            {
                *libc::__errno_location() = self.errno;
            }
        }
        Some(self.result)
    }
}

pub(super) fn write(
    path: &Path,
    bytes: &[u8],
    point: Point,
    result: i32,
    errno: i32,
    attempts: u8,
) -> (Result<(), ProcessError>, Vec<u8>) {
    let (read, report) = UnixStream::pair().unwrap();
    read.set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    let fault = Fault {
        point,
        result,
        errno,
        remaining: attempts,
        report: report.as_raw_fd(),
    };
    let outcome = write_inner(path, bytes, None, Some(fault));
    (outcome, collect(read, report))
}

fn collect(mut read: UnixStream, report: UnixStream) -> Vec<u8> {
    // Our writer has completed/reaped. End this socket direction even if an
    // unrelated concurrent fork still owns a duplicate of its descriptor.
    report.shutdown(std::net::Shutdown::Write).unwrap();
    drop(report);
    let mut consumed = Vec::new();
    read.read_to_end(&mut consumed).unwrap();
    consumed
}

#[test]
fn receipt_completion_does_not_wait_for_an_unrelated_forked_child() {
    use std::io::Write;
    struct Child {
        pid: libc::pid_t,
        release: UnixStream,
    }
    impl Drop for Child {
        fn drop(&mut self) {
            // SAFETY: this byte releases our exclusively owned direct child.
            unsafe {
                libc::write(self.release.as_raw_fd(), b"G".as_ptr().cast(), 1);
            }
            let mut status = 0;
            loop {
                // SAFETY: retain the direct child PID until this owner reaps it.
                let result = unsafe { libc::waitpid(self.pid, &mut status, 0) };
                if result == self.pid {
                    break;
                }
                if std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
                    break;
                }
            }
        }
    }
    let (read, mut report) = UnixStream::pair().unwrap();
    read.set_read_timeout(Some(std::time::Duration::from_secs(1)))
        .unwrap();
    let (release, blocked) = UnixStream::pair().unwrap();
    // SAFETY: the child executes only raw poll/read/_exit on inherited storage.
    let pid = unsafe { libc::fork() };
    assert!(pid >= 0);
    if pid == 0 {
        let fd = blocked.as_raw_fd();
        let mut poll = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let mut byte = [0];
        // SAFETY: all pointers are initialized bounded stack storage. No Drop.
        unsafe {
            let ready = libc::poll(&mut poll, 1, 10_000);
            let received = if ready == 1 {
                libc::read(fd, byte.as_mut_ptr().cast(), 1)
            } else {
                -1
            };
            libc::_exit(if received == 1 && byte == *b"G" {
                0
            } else {
                125
            });
        }
    }
    let child = Child { pid, release };
    drop(blocked);
    report.write_all(b"F").unwrap();
    assert_eq!(collect(read, report), b"F");
    let mut status = 0;
    // SAFETY: merely query our retained child's status without blocking/reaping.
    assert_eq!(unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) }, 0);
    drop(child);
}
