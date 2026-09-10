//! Test-only actual-open synchronization. Rust callbacks run exclusively in parent.
use std::{
    cell::RefCell,
    io::{self, Read, Write},
    os::{fd::AsRawFd, unix::net::UnixStream},
    panic::{AssertUnwindSafe, catch_unwind},
    time::Duration,
};

pub(super) struct Hook<'a> {
    parent: libc::pid_t,
    control: UnixStream,
    child: UnixStream,
    observer: RefCell<&'a mut dyn FnMut()>,
}
impl<'a> Hook<'a> {
    pub(super) fn new(observer: &'a mut dyn FnMut()) -> io::Result<Self> {
        let (control, child) = UnixStream::pair()?;
        control.set_read_timeout(Some(Duration::from_secs(5)))?;
        control.set_write_timeout(Some(Duration::from_secs(1)))?;
        Ok(Self {
            // SAFETY: getpid has no preconditions.
            parent: unsafe { libc::getpid() },
            control,
            child,
            observer: RefCell::new(observer),
        })
    }

    pub(super) fn opened(&self) -> bool {
        // This parent branch deliberately supports the parent-writer mutation:
        // the same hook observes its real writable open, without moving the hook.
        // SAFETY: getpid and the child syscall handshake are async-signal-safe.
        unsafe {
            if libc::getpid() == self.parent {
                (self.observer.borrow_mut())();
                return true;
            }
            let fd = self.child.as_raw_fd();
            if libc::write(fd, b"R".as_ptr().cast(), 1) != 1 {
                return false;
            }
            let mut poll = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };
            if libc::poll(&mut poll, 1, 10_000) != 1 {
                return false;
            }
            let mut byte = [0];
            libc::read(fd, byte.as_mut_ptr().cast(), 1) == 1 && byte == *b"G"
        }
    }

    // Always release the child, including callback panic. The caller reaps it
    // before propagating either the panic or an observation error.
    pub(super) fn observe(&self) -> std::thread::Result<io::Result<()>> {
        let observed = catch_unwind(AssertUnwindSafe(|| {
            let mut control = &self.control;
            let mut ready = [0];
            control.read_exact(&mut ready)?;
            if ready != *b"R" {
                return Err(io::ErrorKind::InvalidData.into());
            }
            (self.observer.borrow_mut())();
            Ok(())
        }));
        let released = (&self.control).write_all(b"G");
        match observed {
            Ok(result) => Ok(result.and(released)),
            Err(panic) => Err(panic),
        }
    }
}
