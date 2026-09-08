//! Host-only terminal ownership. No callbacks call runtime or allocation APIs.
use std::{
    io,
    mem::MaybeUninit,
    sync::atomic::{AtomicI32, Ordering},
};
static TERMINATED: AtomicI32 = AtomicI32::new(0);
extern "C" fn terminate(signal: libc::c_int) {
    TERMINATED.store(signal, Ordering::Relaxed);
}
pub struct Terminal {
    attributes: libc::termios,
    flags: [libc::c_int; 2],
    signals: Vec<(libc::c_int, libc::sigaction)>,
}
fn checked(result: libc::c_int) -> io::Result<libc::c_int> {
    if result < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(result)
    }
}
impl Terminal {
    pub fn enter() -> io::Result<Self> {
        // SAFETY: descriptors 0 and 1 are borrowed process-standard descriptors.
        if unsafe { libc::isatty(0) } != 1 || unsafe { libc::isatty(1) } != 1 {
            return Err(io::Error::other(
                "interactive requires terminal stdin and stdout",
            ));
        }
        // SAFETY: both calls inspect process/terminal group identity without mutation.
        if checked(unsafe { libc::tcgetpgrp(0) })? != unsafe { libc::getpgrp() } {
            return Err(io::Error::other(
                "interactive must own the foreground terminal",
            ));
        }
        let mut attributes = MaybeUninit::uninit();
        // SAFETY: tcgetattr initializes the complete termios on success.
        checked(unsafe { libc::tcgetattr(0, attributes.as_mut_ptr()) })?;
        // SAFETY: successful tcgetattr above initialized this value.
        let attributes = unsafe { attributes.assume_init() };
        // SAFETY: F_GETFL reads descriptor flags and takes no third argument.
        let flags = [
            checked(unsafe { libc::fcntl(0, libc::F_GETFL) })?,
            checked(unsafe { libc::fcntl(1, libc::F_GETFL) })?,
        ];
        let mut owner = Self {
            attributes,
            flags,
            signals: Vec::with_capacity(4),
        };
        for signal in [libc::SIGHUP, libc::SIGTERM, libc::SIGINT, libc::SIGQUIT] {
            // SAFETY: zero initializes valid sigaction storage; sigemptyset then initializes its mask.
            let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
            action.sa_sigaction = terminate as *const () as libc::sighandler_t;
            // SAFETY: mask and old-action storage are valid; handler only stores a lock-free atomic.
            unsafe { libc::sigemptyset(&mut action.sa_mask) };
            let mut old = MaybeUninit::uninit();
            // SAFETY: sigaction copies the action and initializes old on success.
            checked(unsafe { libc::sigaction(signal, &action, old.as_mut_ptr()) })?;
            // SAFETY: successful sigaction initialized old.
            owner.signals.push((signal, unsafe { old.assume_init() }));
        }
        let mut raw = attributes;
        // SAFETY: raw is an initialized termios; cfmakeraw mutates only this local copy.
        unsafe { libc::cfmakeraw(&mut raw) };
        // Let the terminal deliver emergency cancellation independently of input
        // backpressure. NOFLSH preserves already queued bytes until cancellation.
        raw.c_lflag |= libc::ISIG | libc::NOFLSH;
        raw.c_cc[libc::VINTR] = 29;
        raw.c_cc[libc::VQUIT] = libc::_POSIX_VDISABLE;
        raw.c_cc[libc::VSUSP] = libc::_POSIX_VDISABLE;
        #[cfg(target_os = "macos")]
        {
            raw.c_cc[libc::VDSUSP] = libc::_POSIX_VDISABLE;
        }
        // SAFETY: borrowed live terminal and valid attributes; guard restores on every return path.
        checked(unsafe { libc::tcsetattr(0, libc::TCSANOW, &raw) })?;
        for (fd, flags) in owner.flags.iter().enumerate() {
            // SAFETY: same borrowed standard descriptor; original flags retained by guard.
            checked(unsafe { libc::fcntl(fd as i32, libc::F_SETFL, flags | libc::O_NONBLOCK) })?;
        }
        Ok(owner)
    }
    pub fn size(&self) -> io::Result<(u16, u16)> {
        let mut size = MaybeUninit::<libc::winsize>::uninit();
        // SAFETY: ioctl writes a winsize to valid, correctly sized storage.
        checked(unsafe { libc::ioctl(0, libc::TIOCGWINSZ, size.as_mut_ptr()) })?;
        // SAFETY: successful ioctl initialized the winsize.
        let size = unsafe { size.assume_init() };
        Ok((size.ws_col.max(1), size.ws_row.max(1)))
    }
    pub fn signal(&self) -> i32 {
        TERMINATED.load(Ordering::Relaxed)
    }
    pub fn poll(&self, input: bool, output: bool) -> io::Result<bool> {
        let mut descriptors = [
            libc::pollfd {
                fd: 0,
                events: if input { libc::POLLIN } else { 0 },
                revents: 0,
            },
            libc::pollfd {
                fd: 1,
                events: if output { libc::POLLOUT } else { 0 },
                revents: 0,
            },
        ];
        // SAFETY: poll borrows the two-element initialized descriptor array for this call only.
        let result = unsafe {
            libc::poll(
                descriptors.as_mut_ptr(),
                descriptors.len() as libc::nfds_t,
                10,
            )
        };
        if result < 0 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
            return Err(io::Error::last_os_error());
        }
        Ok(descriptors
            .iter()
            .any(|fd| fd.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0))
    }
}
pub fn read(bytes: &mut [u8]) -> io::Result<Option<usize>> {
    // SAFETY: read borrows valid mutable storage; descriptor is not owned or closed here.
    transfer(unsafe { libc::read(0, bytes.as_mut_ptr().cast(), bytes.len()) })
}
pub fn write(bytes: &[u8]) -> io::Result<Option<usize>> {
    // SAFETY: write borrows initialized byte storage for this call only.
    transfer(unsafe { libc::write(1, bytes.as_ptr().cast(), bytes.len()) })
}
fn transfer(result: libc::ssize_t) -> io::Result<Option<usize>> {
    if result >= 0 {
        return Ok(Some(result as usize));
    }
    let error = io::Error::last_os_error();
    if matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
    ) {
        Ok(None)
    } else {
        Err(error)
    }
}
impl Drop for Terminal {
    fn drop(&mut self) {
        // SAFETY: restores only the borrowed terminal state captured on entry. Errors at hangup cannot be repaired.
        unsafe {
            libc::tcsetattr(0, libc::TCSANOW, &self.attributes);
            for (fd, flags) in self.flags.iter().enumerate() {
                libc::fcntl(fd as i32, libc::F_SETFL, *flags);
            }
            for (signal, action) in self.signals.iter().rev() {
                libc::sigaction(*signal, action, std::ptr::null_mut());
            }
        }
    }
}
