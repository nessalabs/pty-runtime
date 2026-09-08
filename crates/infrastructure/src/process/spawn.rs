use super::error;
use pty_runtime_domain::{
    process::{CommandSpec, EnvironmentPolicy, ProcessError},
    terminal::TerminalSize,
};
use std::{
    fs::File,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::process::CommandExt,
    },
    path::PathBuf,
    process::{Child, Command, Stdio},
};
pub(super) fn launch(
    spec: &CommandSpec,
    size: TerminalSize,
    roots: &[LaunchRoot],
) -> Result<(Child, File), ProcessError> {
    let cwd = spec.cwd().canonicalize().map_err(error)?;
    let directory = open_directory(&cwd, roots)?;
    let (host, child) = super::endpoints::open(size)?;
    let mut command = Command::new(spec.executable());
    command.args(spec.arguments());
    if spec.environment() == EnvironmentPolicy::Empty {
        command.env_clear();
    }
    for name in spec.removals() {
        command.env_remove(name);
    }
    for (name, value) in spec.overrides() {
        command.env(name, value);
    }
    command
        .stdin(Stdio::from(child.try_clone().map_err(error)?))
        .stdout(Stdio::from(child.try_clone().map_err(error)?))
        .stderr(Stdio::from(child));
    let dirfd = directory.as_raw_fd();
    // SAFETY: the closure uses only async-signal-safe syscalls, no allocation or locks.
    // Stdio remapping precedes pre_exec; fd 0 is the owned child PTY endpoint.
    unsafe {
        command.pre_exec(move || {
            if libc::fchdir(dirfd) < 0
                || libc::setsid() < 0
                || libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = command.spawn().map_err(error)?;
    Ok((child, host))
}
pub(super) struct LaunchRoot {
    path: PathBuf,
    directory: File,
}
pub(super) fn roots(paths: &[PathBuf]) -> Result<Vec<LaunchRoot>, ProcessError> {
    if paths.is_empty() || paths.len() > 128 {
        return Err(ProcessError::InvalidCommand);
    }
    paths
        .iter()
        .map(|path| {
            let path = path.canonicalize().map_err(error)?;
            let directory = descend(
                File::open("/").map_err(error)?,
                path.strip_prefix("/")
                    .map_err(|_| ProcessError::InvalidCommand)?,
            )?;
            Ok(LaunchRoot { path, directory })
        })
        .collect()
}
fn open_directory(path: &std::path::Path, roots: &[LaunchRoot]) -> Result<File, ProcessError> {
    for root in roots {
        if let Ok(relative) = path.strip_prefix(&root.path) {
            return descend(root.directory.try_clone().map_err(error)?, relative);
        }
    }
    Err(ProcessError::OutsideRoots)
}
fn descend(mut directory: File, relative: &std::path::Path) -> Result<File, ProcessError> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    for component in relative.components() {
        let std::path::Component::Normal(name) = component else {
            return Err(ProcessError::InvalidCommand);
        };
        let name = CString::new(name.as_bytes()).map_err(|_| ProcessError::InvalidCommand)?;
        // SAFETY: live directory and NUL-terminated component; O_NOFOLLOW prevents
        // post-canonicalization symlink replacement from redirecting any path step.
        let fd = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(error(std::io::Error::last_os_error()));
        }
        // SAFETY: successful openat returned a new exclusively owned descriptor.
        directory = unsafe { File::from_raw_fd(fd) };
    }
    Ok(directory)
}
