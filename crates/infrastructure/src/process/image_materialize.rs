//! Materialize only in an owned child: caller forks cannot inherit an image writer.
use pty_runtime_domain::process::ProcessError;
use std::{
    ffi::{CStr, CString},
    mem::MaybeUninit,
    os::unix::ffi::OsStrExt,
    path::Path,
};

#[cfg(test)]
#[path = "image_materialize_hook.rs"]
mod hook;

pub(super) fn write(path: &Path, bytes: &[u8]) -> Result<(), ProcessError> {
    write_inner(
        path,
        bytes,
        #[cfg(test)]
        None,
    )
}

#[cfg(test)]
pub(super) fn write_observed(
    path: &Path,
    bytes: &[u8],
    observer: &mut dyn FnMut(),
) -> Result<(), ProcessError> {
    let hook = hook::Hook::new(observer).map_err(super::error)?;
    write_inner(path, bytes, Some(&hook))
}

fn write_inner(
    path: &Path,
    bytes: &[u8],
    #[cfg(test)] hook: Option<&hook::Hook<'_>>,
) -> Result<(), ProcessError> {
    let path =
        CString::new(path.as_os_str().as_bytes()).map_err(|_| ProcessError::InvalidCommand)?;
    let mut blocked = MaybeUninit::<libc::sigset_t>::uninit();
    let mut previous = MaybeUninit::<libc::sigset_t>::uninit();
    // SAFETY: both outputs are writable sigset_t storage. Block inherited handlers
    // before fork; the child performs only raw operations and keeps this mask.
    let mask_error = unsafe {
        if libc::sigfillset(blocked.as_mut_ptr()) != 0 {
            return Err(super::error(std::io::Error::last_os_error()));
        }
        libc::pthread_sigmask(libc::SIG_SETMASK, blocked.as_ptr(), previous.as_mut_ptr())
    };
    if mask_error != 0 {
        return Err(super::error(std::io::Error::from_raw_os_error(mask_error)));
    }
    // SAFETY: the child uses only inherited immutable byte/path storage and
    // async-signal-safe syscalls, with no allocation, locking, unwinding or Drop.
    // The parent never opens the executable inode for writing.
    let pid = unsafe { libc::fork() };
    if pid == 0 {
        let status = child_write(
            &path,
            bytes,
            #[cfg(test)]
            hook,
        );
        // SAFETY: exit the owned materializer without inherited Rust destructors.
        unsafe { libc::_exit(status) }
    }
    let fork_error = (pid < 0).then(std::io::Error::last_os_error);
    // SAFETY: successful pthread_sigmask initialized previous; only this thread's
    // original mask is restored. Reap a started child even if restoration fails.
    let restore_error = unsafe {
        libc::pthread_sigmask(libc::SIG_SETMASK, previous.as_ptr(), std::ptr::null_mut())
    };
    if let Some(error) = fork_error {
        return Err(super::error(error));
    }
    #[cfg(test)]
    let observed = hook.map(hook::Hook::observe);
    let result = reap(pid);
    #[cfg(test)]
    if let Some(observed) = observed {
        match observed {
            Ok(result) => result.map_err(super::error)?,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }
    if restore_error != 0 {
        return Err(super::error(std::io::Error::from_raw_os_error(
            restore_error,
        )));
    }
    result
}

fn reap(pid: libc::pid_t) -> Result<(), ProcessError> {
    let mut status = 0;
    loop {
        // SAFETY: pid is our successful fork result; status is writable. Host
        // SIGCHLD auto-reaping and competing reapers are prohibited by backend.
        let result = unsafe { libc::waitpid(pid, &mut status, 0) };
        if result == pid {
            // A tracer can report a stop even without WUNTRACED. Keep ownership
            // until waitpid reports an actual exit or fatal signal.
            if libc::WIFEXITED(status) || libc::WIFSIGNALED(status) {
                break;
            }
            continue;
        }
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::Interrupted {
            return Err(super::error(error));
        }
    }
    if !libc::WIFEXITED(status) {
        return Err(ProcessError::Io);
    }
    match libc::WEXITSTATUS(status) {
        0 => Ok(()),
        2 => Err(ProcessError::NotFound),
        3 => Err(ProcessError::PermissionDenied),
        _ => Err(ProcessError::Io),
    }
}

// This function runs after fork. Keep it allocation-free and free of Rust
// destructors; only scalar arithmetic and async-signal-safe libc calls belong here.
fn child_write(
    path: &CStr,
    bytes: &[u8],
    #[cfg(test)] hook: Option<&hook::Hook<'_>>,
) -> libc::c_int {
    // SAFETY: path and bytes are immutable inherited storage; open creates only
    // the preselected private inode. Each write stays within the supplied slice.
    unsafe {
        let fd = libc::open(
            path.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o700,
        );
        if fd < 0 {
            return failure();
        }
        // Observe the actual writable open, never a pre-write approximation.
        #[cfg(test)]
        if let Some(hook) = hook {
            if !hook.opened() {
                libc::close(fd);
                return 1;
            }
        }
        let mut offset = 0;
        while offset < bytes.len() {
            let written = libc::write(fd, bytes.as_ptr().add(offset).cast(), bytes.len() - offset);
            if written < 0 {
                if errno() == libc::EINTR {
                    continue;
                }
                return failure();
            }
            if written == 0 {
                return 1;
            }
            offset += written as usize;
        }
        if libc::fchmod(fd, 0o500) < 0 {
            return failure();
        }
        if libc::close(fd) < 0 {
            return failure();
        }
        0
    }
}
fn failure() -> libc::c_int {
    match errno() {
        libc::ENOENT => 2,
        libc::EACCES | libc::EPERM => 3,
        _ => 1,
    }
}
fn errno() -> libc::c_int {
    // SAFETY: libc returns the current thread's valid errno slot on each platform.
    #[cfg(target_os = "linux")]
    unsafe {
        *libc::__errno_location()
    }
    #[cfg(target_os = "macos")]
    unsafe {
        *libc::__error()
    }
}

#[cfg(test)]
#[path = "../../tests/fixtures/process_image_materialize.rs"]
mod tests;
