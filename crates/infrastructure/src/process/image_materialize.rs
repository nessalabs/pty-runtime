//! Fork-isolated executable writer owned by [`super::HelperImage`] staging.
use pty_runtime_domain::process::ProcessError;
use std::{
    ffi::{CStr, CString},
    mem::MaybeUninit,
    os::unix::ffi::OsStrExt,
    path::Path,
};

#[cfg(test)]
#[path = "../../tests/fixtures/process_image_materialize_hook.rs"]
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
    let hook = hook::Hook::new(observer).map_err(super::super::error)?;
    write_inner(path, bytes, Some(&hook))
}

#[cfg(test)]
pub(super) fn write_abort_after_open(path: &Path, bytes: &[u8]) -> Result<(), ProcessError> {
    let hook = hook::Hook::abort_after_open().map_err(super::super::error)?;
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
            return Err(super::super::error(std::io::Error::last_os_error()));
        }
        libc::pthread_sigmask(libc::SIG_SETMASK, blocked.as_ptr(), previous.as_mut_ptr())
    };
    if mask_error != 0 {
        return Err(super::super::error(std::io::Error::from_raw_os_error(
            mask_error,
        )));
    }
    // SAFETY: the child uses only inherited immutable byte/path storage and
    // async-signal-safe syscalls, with no allocation, locking, unwinding or Drop.
    // The parent never opens the executable inode for writing.
    let pid = unsafe { libc::fork() };
    if pid == 0 {
        #[cfg(test)]
        if let Some(hook) = hook {
            hook.retain_child_end();
        }
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
    #[cfg(test)]
    if let Some(hook) = hook {
        hook.retain_parent_end();
    }
    // Restore before any other parent work. Retry EINTR; surface restore failure
    // even when fork itself failed so this thread cannot keep a full block mask.
    let mut restore_error = restore_mask(&previous);
    if let Some(error) = fork_error {
        if restore_error != 0 {
            restore_error = restore_mask(&previous);
        }
        return Err(super::super::error(if restore_error != 0 {
            std::io::Error::from_raw_os_error(restore_error)
        } else {
            error
        }));
    }
    #[cfg(test)]
    let observed = hook.map(hook::Hook::observe);
    // Child unlinks after a successful open that later fails. Parent only removes
    // when the child may have been killed before that cleanup could run.
    let (result, remove) = reap(pid);
    if remove {
        unlink_path(&path);
    }
    #[cfg(test)]
    if let Some(observed) = observed {
        match observed {
            Ok(result) => result.map_err(super::super::error)?,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }
    if restore_error != 0 {
        restore_error = restore_mask(&previous);
    }
    if restore_error != 0 {
        unlink_path(&path);
        return Err(super::super::error(std::io::Error::from_raw_os_error(
            restore_error,
        )));
    }
    result
}

fn restore_mask(previous: &MaybeUninit<libc::sigset_t>) -> libc::c_int {
    loop {
        // SAFETY: successful install initialized previous; only this thread's mask.
        let error = unsafe {
            libc::pthread_sigmask(libc::SIG_SETMASK, previous.as_ptr(), std::ptr::null_mut())
        };
        if error != libc::EINTR {
            return error;
        }
    }
}

fn unlink_path(path: &CStr) {
    // SAFETY: path is the NUL-terminated destination this call owns for staging.
    let _ = unsafe { libc::unlink(path.as_ptr()) };
}

fn reap(pid: libc::pid_t) -> (Result<(), ProcessError>, bool) {
    let mut status = 0;
    let mut killed = false;
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
        if error.kind() == std::io::ErrorKind::Interrupted {
            continue;
        }
        if !killed {
            // SAFETY: pid is still our unreaped child; SIGKILL ends a stuck writer.
            let _ = unsafe { libc::kill(pid, libc::SIGKILL) };
            killed = true;
            continue;
        }
        // Ownership lost (competing reaper / NOCLDWAIT): best-effort path cleanup.
        return (Err(ProcessError::Io), true);
    }
    if libc::WIFSIGNALED(status) {
        return (Err(ProcessError::Io), true);
    }
    if !libc::WIFEXITED(status) {
        return (Err(ProcessError::Io), true);
    }
    let result = match libc::WEXITSTATUS(status) {
        0 => Ok(()),
        2 => Err(ProcessError::NotFound),
        3 => Err(ProcessError::PermissionDenied),
        _ => Err(ProcessError::Io),
    };
    (result, false)
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
        let fd = open_exclusive(path);
        if fd < 0 {
            return failure();
        }
        // Observe the actual writable open, never a pre-write approximation.
        #[cfg(test)]
        if let Some(hook) = hook {
            if !hook.wait_to_proceed() {
                let _ = libc::close(fd);
                let _ = libc::unlink(path.as_ptr());
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
                return abandon(fd, path);
            }
            if written == 0 {
                let _ = libc::close(fd);
                let _ = libc::unlink(path.as_ptr());
                return 1;
            }
            offset += written as usize;
        }
        if libc::fchmod(fd, 0o500) < 0 {
            return abandon(fd, path);
        }
        if libc::close(fd) < 0 {
            let _ = libc::unlink(path.as_ptr());
            return failure();
        }
        0
    }
}

unsafe fn open_exclusive(path: &CStr) -> libc::c_int {
    loop {
        // SAFETY: caller supplies a NUL-terminated staging path; flags create a
        // new non-followed inode with close-on-exec.
        let fd = unsafe {
            libc::open(
                path.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o700,
            )
        };
        if fd >= 0 || errno() != libc::EINTR {
            return fd;
        }
    }
}

unsafe fn abandon(fd: libc::c_int, path: &CStr) -> libc::c_int {
    let code = failure();
    // SAFETY: fd was opened in this child; path is the matching staging inode.
    let _ = unsafe { libc::close(fd) };
    let _ = unsafe { libc::unlink(path.as_ptr()) };
    code
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
