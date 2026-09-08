use super::error;
use pty_runtime_domain::{
    process::{CommandSpec, EnvironmentPolicy, ProcessError},
    terminal::TerminalSize,
};
use std::{
    fs::File,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::{net::UnixStream, process::CommandExt},
    },
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);
pub(super) fn launch(
    spec: &CommandSpec,
    size: TerminalSize,
    roots: &[LaunchRoot],
    image: &super::image::HelperImage,
    grace: Duration,
) -> Result<(super::guardian::Guardian, File), ProcessError> {
    let cwd = spec.cwd().canonicalize().map_err(error)?;
    let directory = open_directory(&cwd, roots)?;
    let (mut host, child) = super::endpoints::open(size)?;
    let cleanup_host = host.try_clone().map_err(error)?;
    let generation = NEXT_GENERATION
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .map_err(|_| ProcessError::Capacity)?;
    let (owner_s, child_s) = UnixStream::pair().map_err(error)?;
    let (owner_g, child_g) = UnixStream::pair().map_err(error)?;
    let channels = [
        super::protocol::Channel::new(owner_s, generation).map_err(error)?,
        super::protocol::Channel::new(owner_g, generation).map_err(error)?,
    ];
    let copies = [
        copy_fd(directory.as_raw_fd())?,
        copy_fd(child_s.as_raw_fd())?,
        copy_fd(child_g.as_raw_fd())?,
        copy_fd(host.as_raw_fd())?,
        copy_fd(child.as_raw_fd())?,
    ];
    let mut command = Command::new(image.path());
    command
        .arg("--pty-runtime-guardian-v1")
        .arg(generation.to_string())
        .arg(grace.as_millis().to_string())
        .arg(spec.executable())
        .args(spec.arguments());
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
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let dirfd = copies[0].as_raw_fd();
    let mapped = [
        copies[1].as_raw_fd(),
        copies[2].as_raw_fd(),
        copies[3].as_raw_fd(),
        copies[4].as_raw_fd(),
    ];
    // SAFETY: the closure uses only async-signal-safe syscalls, no allocation or locks.
    // All sources are above the fixed destinations, including the pinned cwd;
    // stdio remapping and closed host fd 0/1/2 cannot overwrite those sources.
    unsafe {
        command.pre_exec(move || {
            if libc::fchdir(dirfd) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            for (index, source) in mapped.iter().enumerate() {
                if libc::dup2(*source, 3 + index as i32) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    let sentinel = command.spawn().map_err(error)?;
    // Parent copies of child channel endpoints would suppress EOF during failed
    // admission. Close them before constructing the protocol cleanup owner.
    drop(copies);
    drop(child_s);
    drop(child_g);
    drop(child);
    let mut guardian = super::guardian::Guardian::new(sentinel, channels, generation, cleanup_host);
    guardian.admit(&mut host)?;
    Ok((guardian, host))
}
fn copy_fd(fd: i32) -> Result<OwnedFd, ProcessError> {
    // SAFETY: fd is borrowed live; fcntl returns a new exclusively owned
    // close-on-exec duplicate outside the fixed helper/stdio mapping range.
    let copy = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 16) };
    if copy < 0 {
        return Err(error(std::io::Error::last_os_error()));
    }
    // SAFETY: successful fcntl transferred ownership of this fresh descriptor.
    Ok(unsafe { OwnedFd::from_raw_fd(copy) })
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
