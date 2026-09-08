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
    time::{Duration, Instant},
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
        let (parent, name) = parent_and_name(path)?;
        // Also coordinate an explicit new(path) inside an adapter arena: no
        // reclaimer may mistake mkdir-before-owner-lock for an abandoned owner.
        lock_bounded(&parent)?;
        let result = Self::at(parent, name, true);
        if let Ok(directory) = &result {
            // SAFETY: parent is still owned; this releases only the short
            // construction lock, never the namespace root's separate flock.
            unsafe {
                libc::flock(directory.parent.as_raw_fd(), libc::LOCK_UN);
            }
        }
        result
    }
    pub fn existing(path: &Path) -> Result<Self, CheckpointError> {
        let (parent, name) = parent_and_name(path)?;
        Self::at(parent, name, false)
    }
    pub fn child(&self, name: &CStr, create: bool) -> Result<Self, CheckpointError> {
        // A new open description avoids inheriting an arena flock through dup.
        let parent = open_directory(self.root.as_raw_fd(), c".")?;
        Self::at(parent, name.to_owned(), create)
    }
    fn at(parent: File, name: CString, create: bool) -> Result<Self, CheckpointError> {
        // SAFETY: parent is owned and name remains NUL-terminated during mkdirat.
        if create && unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o700) } < 0 {
            return Err(map(io::Error::last_os_error()));
        }
        let mut pending = ConstructionGuard {
            parent: &parent,
            name: &name,
            armed: create,
        };
        let root = open_directory(parent.as_raw_fd(), &name)?;
        if create && !try_lock(&root)? {
            return Err(CheckpointError::Unavailable);
        }
        pending.armed = false;
        drop(pending);
        Ok(Self { root, parent, name })
    }
    pub fn try_lock(&self) -> Result<bool, CheckpointError> {
        try_lock(&self.root)
    }
    pub fn lock_bounded(&self) -> Result<(), CheckpointError> {
        lock_bounded(&self.root)
    }
    pub fn entries(&self) -> Result<super::inventory::Entries, CheckpointError> {
        super::inventory::Entries::new(self.root.as_raw_fd())
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
        if !metadata.is_file() || metadata.mode() & 0o777 != 0o600 || metadata.nlink() != 1
            // SAFETY: geteuid is a read-only scalar process identity query.
            || metadata.uid() != unsafe { libc::geteuid() }
        {
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
fn parent_and_name(path: &Path) -> Result<(File, CString), CheckpointError> {
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
    Ok((parent, name))
}
fn open_directory(parent: i32, name: &CStr) -> Result<File, CheckpointError> {
    // SAFETY: parent/name are live borrowed inputs; openat transfers a new FD.
    let fd = unsafe {
        libc::openat(
            parent,
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(map(io::Error::last_os_error()));
    }
    // SAFETY: successful openat returned a fresh exclusively owned descriptor.
    let file = unsafe { File::from_raw_fd(fd) };
    let info = file.metadata().map_err(map)?;
    // SAFETY: geteuid is a read-only scalar process identity query.
    if info.mode() & 0o777 != 0o700 || info.uid() != unsafe { libc::geteuid() } {
        return Err(CheckpointError::InvalidConfiguration);
    }
    Ok(file)
}
fn try_lock(file: &File) -> Result<bool, CheckpointError> {
    // SAFETY: flock affects this owned open description and retains no pointers.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    if error.kind() == io::ErrorKind::WouldBlock {
        Ok(false)
    } else {
        Err(map(error))
    }
}
fn lock_bounded(file: &File) -> Result<(), CheckpointError> {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        if try_lock(file)? {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(CheckpointError::CapacityExceeded);
        }
        std::thread::sleep(Duration::from_millis(1));
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
