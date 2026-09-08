use super::filesystem::{Directory, map};
use pty_runtime_application::checkpoint::ICheckpointStore;
use pty_runtime_domain::{checkpoint::*, terminal::CheckpointDescriptor};
use std::{
    collections::HashMap,
    io::{Read, Write},
    path::Path,
    sync::Mutex,
};

struct Entry {
    reference: CheckpointRef,
    descriptor: CheckpointDescriptor,
    name: String,
}
struct State {
    entries: HashMap<CheckpointKey, Entry>,
    committed: usize,
    inflight: usize,
    abandoned: usize,
    sequence: u64,
    orphans: Vec<String>,
}
/// Private owner-lifetime disk store. Creates a fresh 0700 namespace; rejects existing
/// roots (including symlinks). Child operations use an anchored directory descriptor.
/// Storage operations serialize on a dedicated store mutex, never a native model lock.
/// Quota counts logical ciphertext bytes in temporary and committed files exactly once;
/// it excludes filesystem block rounding/cache and caller-owned encryption buffers.
/// Drop deletes owned objects best-effort; crash recovery is deliberately unsupported.
pub struct FileCheckpointStore {
    directory: Directory,
    limit: usize,
    state: Mutex<State>,
}
impl FileCheckpointStore {
    /// Create a new private directory at `path`; parent must already exist and remain
    /// trusted against concurrent name replacement through this store's lifetime.
    /// Drop refuses a preexisting replacement namespace; Unix cannot atomically
    /// condition directory removal on inode identity during hostile concurrent renames.
    /// The runtime supplies a unique path for its lifetime. Bounds must be finite/nonzero.
    pub fn new(path: impl AsRef<Path>, limit_bytes: usize) -> Result<Self, CheckpointError> {
        if limit_bytes == 0 {
            return Err(CheckpointError::InvalidConfiguration);
        }
        let path = path.as_ref().to_owned();
        let directory = Directory::create(&path)?;
        Ok(Self {
            directory,
            limit: limit_bytes,
            state: Mutex::new(State {
                entries: HashMap::new(),
                committed: 0,
                inflight: 0,
                abandoned: 0,
                sequence: 0,
                orphans: Vec::new(),
            }),
        })
    }
    fn commit_with(
        &self,
        checkpoint: &ProtectedCheckpoint,
        write: impl FnOnce(&mut std::fs::File, &[u8]) -> std::io::Result<()>,
    ) -> Result<CheckpointRef, CheckpointError> {
        self.commit_with_cleanup(checkpoint, write, |name| self.directory.remove(name))
    }
    fn commit_with_cleanup(
        &self,
        checkpoint: &ProtectedCheckpoint,
        write: impl FnOnce(&mut std::fs::File, &[u8]) -> std::io::Result<()>,
        cleanup: impl FnOnce(&str) -> Result<(), CheckpointError>,
    ) -> Result<CheckpointRef, CheckpointError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| CheckpointError::Unavailable)?;
        if state.entries.contains_key(&checkpoint.key) {
            return Err(CheckpointError::AlreadyExists);
        }
        let bytes = checkpoint.ciphertext().len();
        if bytes == 0 || checkpoint.descriptor.compatibility.len() > 4096 {
            return Err(CheckpointError::InvalidConfiguration);
        }
        if bytes > self.limit.saturating_sub(state.committed + state.abandoned) {
            return Err(CheckpointError::CapacityExceeded);
        }
        state.sequence = state
            .sequence
            .checked_add(1)
            .ok_or(CheckpointError::CapacityExceeded)?;
        let mut object_id = [0; 16];
        getrandom::getrandom(&mut object_id).map_err(|_| CheckpointError::EntropyUnavailable)?;
        let name = format!("{:016x}.checkpoint", state.sequence);
        let temp = format!("{:016x}.pending", state.sequence);
        state.inflight = bytes;
        let outcome = (|| {
            let mut file = self.directory.open(&temp, true)?;
            write(&mut file, checkpoint.ciphertext()).map_err(map)?;
            file.sync_all().map_err(map)?;
            self.directory.rename(&temp, &name)?;
            // No fallible operations after atomic publication: an error must not hide a commit.
            Ok(())
        })();
        state.inflight = 0;
        if let Err(error) = outcome {
            // A failed cleanup must remain charged so subsequent writes cannot bypass quota.
            if cleanup(&temp).is_err() {
                state.abandoned += bytes;
                state.orphans.push(temp);
            }
            return Err(error);
        }
        let reference = CheckpointRef {
            key: checkpoint.key,
            bytes,
            object_id,
        };
        state.entries.insert(
            checkpoint.key,
            Entry {
                reference,
                descriptor: checkpoint.descriptor.clone(),
                name,
            },
        );
        state.committed += bytes;
        Ok(reference)
    }
}
impl ICheckpointStore for FileCheckpointStore {
    fn commit(&self, checkpoint: &ProtectedCheckpoint) -> Result<CheckpointRef, CheckpointError> {
        self.commit_with(checkpoint, |file, bytes| file.write_all(bytes))
    }
    fn read(
        &self,
        reference: CheckpointRef,
        max_bytes: usize,
    ) -> Result<ProtectedCheckpoint, CheckpointError> {
        if reference.bytes > max_bytes {
            return Err(CheckpointError::CapacityExceeded);
        }
        let state = self
            .state
            .lock()
            .map_err(|_| CheckpointError::Unavailable)?;
        let entry = state
            .entries
            .get(&reference.key)
            .filter(|e| e.reference == reference)
            .ok_or(CheckpointError::NotFound)?;
        let mut file = self.directory.open(&entry.name, false)?;
        if file.metadata().map_err(map)?.len() != reference.bytes as u64 {
            return Err(CheckpointError::Unavailable);
        }
        let mut bytes = vec![0; reference.bytes];
        file.read_exact(&mut bytes).map_err(map)?;
        let mut extra = [0];
        if file.read(&mut extra).map_err(map)? != 0 {
            return Err(CheckpointError::Unavailable);
        }
        Ok(ProtectedCheckpoint::new(
            reference.key,
            entry.descriptor.clone(),
            bytes,
        ))
    }
    fn delete(&self, reference: CheckpointRef) -> Result<(), CheckpointError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| CheckpointError::Unavailable)?;
        if let Some(entry) = state.entries.get(&reference.key) {
            if entry.reference != reference {
                return Err(CheckpointError::NotFound);
            }
            self.directory.remove(&entry.name)?;
            state.entries.remove(&reference.key);
            state.committed -= reference.bytes;
        }
        Ok(())
    }
    fn capacity(&self) -> CheckpointCapacity {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        CheckpointCapacity {
            limit_bytes: self.limit,
            committed_bytes: state.committed,
            inflight_bytes: state.inflight,
            abandoned_bytes: state.abandoned,
        }
    }
}
impl Drop for FileCheckpointStore {
    fn drop(&mut self) {
        let state = self.state.get_mut().unwrap_or_else(|e| e.into_inner());
        for entry in state.entries.values() {
            let _ = self.directory.remove(&entry.name);
        }
        for name in &state.orphans {
            let _ = self.directory.remove(name);
        }
        // Verify namespace identity through the anchored parent before removing it.
        let _ = self.directory.remove_namespace();
    }
}

#[cfg(test)]
#[path = "file_tests.rs"]
mod tests;
