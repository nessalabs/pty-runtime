//! Test-only actual-open synchronization. Rust callbacks run exclusively in parent.
use std::{
    cell::RefCell,
    io::{self, Read, Write},
    os::{fd::AsRawFd, unix::net::UnixStream},
    panic::{AssertUnwindSafe, catch_unwind},
    time::Duration,
};

enum Release {
    Go,
    Abort,
}

pub(super) struct Hook<'a> {
    control: RefCell<Option<UnixStream>>,
    child: RefCell<Option<UnixStream>>,
    observer: RefCell<Option<&'a mut dyn FnMut()>>,
    release: Release,
}

impl<'a> Hook<'a> {
    pub(super) fn new(observer: &'a mut dyn FnMut()) -> io::Result<Self> {
        Self::pair(Some(observer), Release::Go)
    }

    pub(super) fn abort_after_open() -> io::Result<Self> {
        Self::pair(None, Release::Abort)
    }

    fn pair(observer: Option<&'a mut dyn FnMut()>, release: Release) -> io::Result<Self> {
        let (control, child) = UnixStream::pair()?;
        control.set_read_timeout(Some(Duration::from_secs(5)))?;
        control.set_write_timeout(Some(Duration::from_secs(1)))?;
        Ok(Self {
            control: RefCell::new(Some(control)),
            child: RefCell::new(Some(child)),
            observer: RefCell::new(observer),
            release,
        })
    }

    /// Parent keeps only the control end so child death is observable as hangup.
    pub(super) fn retain_parent_end(&self) {
        drop(self.child.borrow_mut().take());
    }

    /// Child keeps only its end so parent death is observable as hangup.
    pub(super) fn retain_child_end(&self) {
        drop(self.control.borrow_mut().take());
    }

    /// Child-only gate after the writable open. Never runs Rust callbacks here.
    pub(super) fn wait_to_proceed(&self) -> bool {
        let child = self.child.borrow();
        let Some(child) = child.as_ref() else {
            return false;
        };
        // SAFETY: write/poll/read on the inherited child socket are async-signal-safe.
        unsafe {
            let fd = child.as_raw_fd();
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
            if libc::read(fd, byte.as_mut_ptr().cast(), 1) != 1 {
                return false;
            }
            byte == *b"G"
        }
    }

    // Always release the child, including callback panic. The caller reaps it
    // before propagating either the panic or an observation error.
    pub(super) fn observe(&self) -> std::thread::Result<io::Result<()>> {
        let release = match self.release {
            Release::Go => b"G",
            Release::Abort => b"A",
        };
        let observed = catch_unwind(AssertUnwindSafe(|| {
            let mut control = self.control.borrow_mut();
            let control = control
                .as_mut()
                .ok_or_else(|| io::Error::other("materialize hook lost parent control"))?;
            let mut ready = [0];
            control.read_exact(&mut ready)?;
            if ready != *b"R" {
                return Err(io::ErrorKind::InvalidData.into());
            }
            if let Some(observer) = self.observer.borrow_mut().as_mut() {
                observer();
            }
            Ok(())
        }));
        let released = {
            let mut control = self.control.borrow_mut();
            control
                .as_mut()
                .map(|control| control.write_all(release))
                .unwrap_or(Ok(()))
        };
        match observed {
            Ok(result) => Ok(result.and(released)),
            Err(panic) => Err(panic),
        }
    }
}
