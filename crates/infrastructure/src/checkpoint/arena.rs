//! Private shared parent; its short lock serializes initialization with reclamation.
use super::filesystem::Directory;
use pty_runtime_domain::checkpoint::CheckpointError;
use std::{ffi::CString, path::Path};

pub(super) struct Arena(pub Directory);
impl Arena {
    pub fn open(parent: Option<&Path>) -> Result<Self, CheckpointError> {
        let parent = parent
            .map(Path::to_path_buf)
            .unwrap_or_else(std::env::temp_dir);
        // SAFETY: geteuid only reads the scalar effective owner identity.
        let path = parent.join(format!(".pty-runtime-checkpoints-v1-{}", unsafe {
            libc::geteuid()
        }));
        let directory = match Directory::create(&path) {
            Ok(directory) => return Ok(Self(directory)),
            Err(CheckpointError::AlreadyExists) => Directory::existing(&path)?,
            Err(error) => return Err(error),
        };
        // A stalled competing maintenance pass cannot block a constructor forever.
        // Namespace owners never retain this arena lock after construction.
        directory.lock_bounded()?;
        Ok(Self(directory))
    }
    pub fn create_namespace(&self) -> Result<Directory, CheckpointError> {
        for _ in 0..8 {
            let mut random = [0; 16];
            getrandom::getrandom(&mut random).map_err(|_| CheckpointError::EntropyUnavailable)?;
            let name = CString::new(format!("owner-{:032x}", u128::from_ne_bytes(random)))
                .map_err(|_| CheckpointError::InvalidConfiguration)?;
            match self.0.child(&name, true) {
                Err(CheckpointError::AlreadyExists) => continue,
                result => return result,
            }
        }
        Err(CheckpointError::AlreadyExists)
    }
}
pub(super) fn namespace_name(bytes: &[u8]) -> bool {
    bytes.len() == 38
        && bytes.starts_with(b"owner-")
        && bytes[6..]
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
}
