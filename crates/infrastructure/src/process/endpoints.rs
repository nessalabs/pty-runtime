use super::error;
use pty_runtime_domain::{process::ProcessError, terminal::TerminalSize};
use std::{
    fs::File,
    os::fd::{AsRawFd, FromRawFd},
};
/// Allocate with close-on-exec atomically, including concurrent host launches.
pub(super) fn open(size: TerminalSize) -> Result<(File, File), ProcessError> {
    // SAFETY: posix_openpt returns a new descriptor; integer flags are valid.
    let host = unsafe { libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC) };
    if host < 0 {
        return Err(error(std::io::Error::last_os_error()));
    }
    // SAFETY: successful posix_openpt transferred exclusive descriptor ownership.
    let host = unsafe { File::from_raw_fd(host) };
    // SAFETY: the owned host endpoint is valid through both initialization calls.
    if unsafe { libc::grantpt(host.as_raw_fd()) } < 0
        || unsafe { libc::unlockpt(host.as_raw_fd()) } < 0
    {
        return Err(error(std::io::Error::last_os_error()));
    }
    let mut name = [0 as libc::c_char; 128];
    #[cfg(target_os = "linux")]
    // SAFETY: the output array is writable for its full declared length.
    let named = unsafe { libc::ptsname_r(host.as_raw_fd(), name.as_mut_ptr(), name.len()) };
    #[cfg(target_os = "macos")]
    // SAFETY: Darwin TIOCPTYGNAME writes exactly a 128-byte name buffer.
    // Constant is _IOC(IOC_OUT, 't', 83, 128), from the macOS sys/ttycom.h ABI.
    let named = unsafe {
        libc::ioctl(
            host.as_raw_fd(),
            0x4080_7453 as libc::c_ulong,
            name.as_mut_ptr(),
        )
    };
    if named != 0 || !name.contains(&0) {
        return Err(ProcessError::Io);
    }
    // SAFETY: the initialized name is NUL terminated; successful open returns owned fd.
    let child = unsafe {
        libc::open(
            name.as_ptr(),
            libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC,
        )
    };
    if child < 0 {
        return Err(error(std::io::Error::last_os_error()));
    }
    // SAFETY: successful open transferred exclusive descriptor ownership.
    let child = unsafe { File::from_raw_fd(child) };
    // SAFETY: zeroed termios is a valid writable destination for tcgetattr.
    let mut termios: libc::termios = unsafe { std::mem::zeroed() };
    // SAFETY: live child endpoint and valid mutable termios storage.
    if unsafe { libc::tcgetattr(child.as_raw_fd(), &mut termios) } < 0 {
        return Err(error(std::io::Error::last_os_error()));
    }
    termios.c_lflag &= !(libc::ECHO | libc::ECHONL);
    // SAFETY: initialized termios belongs to this live child endpoint.
    if unsafe { libc::tcsetattr(child.as_raw_fd(), libc::TCSANOW, &termios) } < 0 {
        return Err(error(std::io::Error::last_os_error()));
    }
    resize(&host, size)?;
    // SAFETY: host descriptor is live; change open-file status to nonblocking.
    if unsafe { libc::fcntl(host.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) } < 0 {
        return Err(error(std::io::Error::last_os_error()));
    }
    Ok((host, child))
}
pub(super) fn resize(file: &File, size: TerminalSize) -> Result<(), ProcessError> {
    let dimensions = libc::winsize {
        ws_row: size.rows(),
        ws_col: size.cols(),
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: file and dimensions remain valid for this synchronous ioctl.
    if unsafe { libc::ioctl(file.as_raw_fd(), libc::TIOCSWINSZ as _, &dimensions) } < 0 {
        Err(error(std::io::Error::last_os_error()))
    } else {
        Ok(())
    }
}
