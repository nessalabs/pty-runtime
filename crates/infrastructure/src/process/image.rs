//! Immutable executable staging; admission never depends on a runtime compiler.
use pty_runtime_domain::process::ProcessError;
use std::{
    fs::{self, DirBuilder, OpenOptions},
    io::Read,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

#[path = "image_materialize.rs"]
mod materialize;

const IMAGE: &[u8] = include_bytes!(env!("PTY_RUNTIME_GUARDIAN_IMAGE_PATH"));
pub(super) struct HelperImage {
    directory: PathBuf,
    executable: PathBuf,
}
impl HelperImage {
    pub fn new(bundled: Option<&Path>) -> Result<Self, ProcessError> {
        Self::stage(
            bundled,
            #[cfg(test)]
            None,
        )
    }
    fn stage(
        bundled: Option<&Path>,
        #[cfg(test)] mut staging_observer: Option<&mut dyn FnMut()>,
    ) -> Result<Self, ProcessError> {
        if let Some(path) = bundled {
            // Verify a bounded byte-for-byte copy, not only a pathname or mtime.
            // The selected image may already carry its application's signature;
            // staging preserves those exact bytes.
            let mut file = OpenOptions::new()
                .read(true)
                // Open unsupported stream types without waiting for a peer;
                // the metadata check below admits regular files only.
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
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
            #[cfg(test)]
            if let Some(observer) = staging_observer.as_mut() {
                materialize::write_observed(&image.executable, IMAGE, observer)?;
            } else {
                materialize::write(&image.executable, IMAGE)?;
            }
            #[cfg(not(test))]
            materialize::write(&image.executable, IMAGE)?;
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

#[cfg(test)]
#[path = "../../tests/fixtures/process_image_fork.rs"]
mod fork_tests;
