use pty_runtime_domain::checkpoint::CheckpointError;
use std::{
    ffi::{CStr, CString},
    fs::{File, OpenOptions},
    io,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            ffi::OsStrExt,
            fs::{MetadataExt, OpenOptionsExt},
        },
    },
    path::Path,
};

pub(super) struct Directory {
    root: File,
    parent: File,
    name: CString,
}
// Own the just-created namespace before any fallible descriptor/metadata work.
// As with normal namespace removal, its parent must remain trusted against
// concurrent replacement from mkdirat until construction has finished.
struct ConstructionGuard<'a> {
    parent: &'a File,
    name: &'a CStr,
    armed: bool,
}
impl Drop for ConstructionGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            // SAFETY: both borrowed values outlive this guard; unlinkat retains no pointers.
            unsafe {
                libc::unlinkat(
                    self.parent.as_raw_fd(),
                    self.name.as_ptr(),
                    libc::AT_REMOVEDIR,
                );
            }
        }
    }
}
impl Directory {
    pub fn create(path: &Path) -> Result<Self, CheckpointError> {
        let parent_path = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let name = CString::new(
            path.file_name()
                .ok_or(CheckpointError::InvalidConfiguration)?
                .as_bytes(),
        )
        .map_err(|_| CheckpointError::InvalidConfiguration)?;
        let parent = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(parent_path)
            .map_err(map)?;
        // SAFETY: parent is owned and name remains NUL-terminated during mkdirat.
        if unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o700) } < 0 {
            return Err(map(io::Error::last_os_error()));
        }
        let mut pending = ConstructionGuard {
            parent: &parent,
            name: &name,
            armed: true,
        };
        // SAFETY: descriptor/name remain valid and no pointers are retained.
        let fd = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(map(io::Error::last_os_error()));
        }
        // SAFETY: openat returned one newly owned descriptor.
        let root = unsafe { File::from_raw_fd(fd) };
        let info = root.metadata().map_err(map)?;
        // SAFETY: geteuid has no arguments or memory ownership requirements.
        if info.mode() & 0o777 != 0o700 || info.uid() != unsafe { libc::geteuid() } {
            return Err(CheckpointError::InvalidConfiguration);
        }
        pending.armed = false;
        drop(pending);
        Ok(Self { root, parent, name })
    }
    pub fn remove_namespace(&self) -> Result<(), CheckpointError> {
        let owned = self.root.metadata().map_err(map)?;
        let mut named = std::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY: parent/name are owned, and fstatat initializes the output on success.
        if unsafe {
            libc::fstatat(
                self.parent.as_raw_fd(),
                self.name.as_ptr(),
                named.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } < 0
        {
            return Err(map(io::Error::last_os_error()));
        }
        // SAFETY: successful fstatat above initialized the full stat value.
        let named = unsafe { named.assume_init() };
        // libc device/inode widths differ between macOS and Linux.
        #[allow(clippy::unnecessary_cast)]
        let same_identity =
            named.st_dev as u64 == owned.dev() && named.st_ino as u64 == owned.ino();
        if !same_identity {
            return Err(CheckpointError::Unavailable);
        }
        // The parent must remain trusted against concurrent name replacement. Unix has
        // no portable inode-conditional rmdir; refuse an already replaced namespace.
        // SAFETY: valid anchored parent and NUL-terminated name; no pointers retained.
        if unsafe {
            libc::unlinkat(
                self.parent.as_raw_fd(),
                self.name.as_ptr(),
                libc::AT_REMOVEDIR,
            )
        } < 0
        {
            return Err(map(io::Error::last_os_error()));
        }
        Ok(())
    }
    pub fn open(&self, name: &str, write: bool) -> Result<File, CheckpointError> {
        let name = CString::new(name).map_err(|_| CheckpointError::InvalidConfiguration)?;
        let flags = libc::O_CLOEXEC
            | libc::O_NOFOLLOW
            | if write {
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL
            } else {
                libc::O_RDONLY | libc::O_NONBLOCK
            };
        // SAFETY: the owned directory descriptor stays valid; name is NUL-terminated.
        let fd = unsafe { libc::openat(self.root.as_raw_fd(), name.as_ptr(), flags, 0o600) };
        if fd < 0 {
            return Err(map(io::Error::last_os_error()));
        }
        // SAFETY: openat returned a newly owned file descriptor exactly once.
        let file = unsafe { File::from_raw_fd(fd) };
        let metadata = file.metadata().map_err(map)?;
        if !metadata.is_file() || metadata.mode() & 0o777 != 0o600 || metadata.nlink() != 1 {
            return Err(CheckpointError::Unavailable);
        }
        Ok(file)
    }
    pub fn rename(&self, old: &str, new: &str) -> Result<(), CheckpointError> {
        let old = CString::new(old).map_err(|_| CheckpointError::InvalidConfiguration)?;
        let new = CString::new(new).map_err(|_| CheckpointError::InvalidConfiguration)?;
        // SAFETY: both names and the owned directory descriptor remain valid throughout.
        if unsafe {
            libc::renameat(
                self.root.as_raw_fd(),
                old.as_ptr(),
                self.root.as_raw_fd(),
                new.as_ptr(),
            )
        } < 0
        {
            return Err(map(io::Error::last_os_error()));
        }
        Ok(())
    }
    pub fn remove(&self, name: &str) -> Result<(), CheckpointError> {
        let name = CString::new(name).map_err(|_| CheckpointError::InvalidConfiguration)?;
        // SAFETY: descriptor and NUL-terminated child name remain valid; no pointers retained.
        if unsafe { libc::unlinkat(self.root.as_raw_fd(), name.as_ptr(), 0) } < 0 {
            let err = io::Error::last_os_error();
            if err.kind() != io::ErrorKind::NotFound {
                return Err(map(err));
            }
        }
        Ok(())
    }
}
pub(super) fn map(error: io::Error) -> CheckpointError {
    match error.raw_os_error() {
        Some(libc::ENOSPC) | Some(libc::EDQUOT) => CheckpointError::CapacityExceeded,
        _ => match error.kind() {
            io::ErrorKind::NotFound => CheckpointError::NotFound,
            io::ErrorKind::AlreadyExists => CheckpointError::AlreadyExists,
            _ => CheckpointError::Unavailable,
        },
    }
}
