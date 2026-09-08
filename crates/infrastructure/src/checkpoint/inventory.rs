//! Descriptor-anchored, incremental directory enumeration with error-aware EOF.
use super::filesystem::map;
use pty_runtime_domain::checkpoint::CheckpointError;
use std::{
    ffi::{CStr, CString},
    io,
    ptr::NonNull,
};

pub(super) struct Entries(NonNull<libc::DIR>);
impl Entries {
    pub fn new(directory: i32) -> Result<Self, CheckpointError> {
        // SAFETY: a fresh open description preserves the caller's flock/offset
        // independently; fdopendir takes exclusive ownership of that new FD.
        let fd = unsafe {
            libc::openat(
                directory,
                c".".as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            )
        };
        if fd < 0 {
            return Err(map(io::Error::last_os_error()));
        }
        // SAFETY: fd is newly owned and a valid directory descriptor.
        let stream = unsafe { libc::fdopendir(fd) };
        if let Some(stream) = NonNull::new(stream) {
            return Ok(Self(stream));
        }
        let error = io::Error::last_os_error();
        // SAFETY: failed fdopendir did not take ownership of fd.
        unsafe {
            libc::close(fd);
        }
        Err(map(error))
    }
    pub fn next(&mut self) -> Result<Option<CString>, CheckpointError> {
        loop {
            // SAFETY: errno is thread-local; readdir owns this private stream.
            unsafe {
                *errno() = 0;
            }
            let entry = unsafe { libc::readdir(self.0.as_ptr()) };
            if entry.is_null() {
                let error = io::Error::last_os_error();
                return if error.raw_os_error() == Some(0) {
                    Ok(None)
                } else {
                    Err(map(error))
                };
            }
            // SAFETY: readdir returned a valid entry until the next stream call;
            // d_name is NUL terminated and copied before any following call.
            let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
            if name != c"." && name != c".." {
                return Ok(Some(name.to_owned()));
            }
        }
    }
}
impl Drop for Entries {
    fn drop(&mut self) {
        // SAFETY: this is the sole owner of this directory stream and its FD.
        unsafe {
            libc::closedir(self.0.as_ptr());
        }
    }
}
unsafe fn errno() -> *mut libc::c_int {
    #[cfg(target_os = "macos")]
    {
        unsafe { libc::__error() }
    }
    #[cfg(target_os = "linux")]
    {
        unsafe { libc::__errno_location() }
    }
}
