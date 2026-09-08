//! Immutable executable staging; admission never depends on a runtime compiler.
use pty_runtime_domain::process::ProcessError;
use std::{
    fs::{self, DirBuilder, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

const IMAGE: &[u8] = include_bytes!(env!("PTY_RUNTIME_GUARDIAN_IMAGE_PATH"));
pub(super) struct HelperImage {
    directory: PathBuf,
    executable: PathBuf,
}
impl HelperImage {
    pub fn new(bundled: Option<&Path>) -> Result<Self, ProcessError> {
        if let Some(path) = bundled {
            // Verify a bounded byte-for-byte copy, not only a pathname or mtime.
            // The selected image may already carry its application's signature;
            // staging preserves those exact bytes.
            let mut file = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(path)
                .map_err(super::error)?;
            if !file.metadata().map_err(super::error)?.is_file() {
                return Err(ProcessError::InvalidCommand);
            }
            let mut bytes = Vec::with_capacity(IMAGE.len() + 1);
            (&mut file)
                .take(IMAGE.len() as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(super::error)?;
            if bytes != IMAGE {
                return Err(ProcessError::Unsupported);
            }
        }
        for _ in 0..8 {
            let mut random = [0; 16];
            getrandom::getrandom(&mut random).map_err(|_| ProcessError::Io)?;
            let suffix: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
            let directory = std::env::temp_dir().join(format!("pty-guardian-{suffix}"));
            match DirBuilder::new().mode(0o700).create(&directory) {
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(super::error(error)),
                Ok(()) => {}
            }
            let image = Self {
                executable: directory.join("guardian"),
                directory,
            };
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o700)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(&image.executable)
                .map_err(super::error)?;
            file.write_all(IMAGE).map_err(super::error)?;
            file.set_permissions(fs::Permissions::from_mode(0o500))
                .map_err(super::error)?;
            return Ok(image);
        }
        Err(ProcessError::Capacity)
    }
    pub fn path(&self) -> &Path {
        &self.executable
    }
}
impl Drop for HelperImage {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.executable);
        let _ = fs::remove_dir(&self.directory);
    }
}
