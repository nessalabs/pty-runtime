use crate::checkpoint::{ICheckpointProtector, ICheckpointStore};
use pty_runtime_domain::{checkpoint::*, terminal::*};
use std::{
    collections::HashMap,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
#[derive(Default)]
pub struct Store {
    pub entries: Mutex<HashMap<CheckpointKey, (CheckpointRef, ProtectedCheckpoint)>>,
    pub commits: AtomicUsize,
    pub deletes: AtomicUsize,
    pub fail_commit: AtomicBool,
    pub panic_commit: AtomicBool,
    pub invalid_reference: AtomicBool,
    pub fail_read: AtomicBool,
    pub fail_delete: AtomicBool,
}
impl ICheckpointStore for Store {
    fn commit(&self, checkpoint: &ProtectedCheckpoint) -> Result<CheckpointRef, CheckpointError> {
        let id = self.commits.fetch_add(1, Ordering::AcqRel) + 1;
        if self.fail_commit.load(Ordering::Acquire) {
            return Err(CheckpointError::Unavailable);
        }
        let mut object_id = [0; 16];
        object_id[..8].copy_from_slice(&(id as u64).to_le_bytes());
        let reference = CheckpointRef {
            key: checkpoint.key,
            bytes: checkpoint.ciphertext().len(),
            object_id,
        };
        self.entries
            .lock()
            .unwrap()
            .insert(checkpoint.key, (reference, checkpoint.clone()));
        assert!(
            !self.panic_commit.load(Ordering::Acquire),
            "commit panicked after publication"
        );
        if self.invalid_reference.load(Ordering::Acquire) {
            return Ok(CheckpointRef {
                bytes: reference.bytes + 1,
                ..reference
            });
        }
        Ok(reference)
    }
    fn read(
        &self,
        reference: CheckpointRef,
        max: usize,
    ) -> Result<ProtectedCheckpoint, CheckpointError> {
        if self.fail_read.load(Ordering::Acquire) {
            return Err(CheckpointError::Unavailable);
        }
        if reference.bytes > max {
            return Err(CheckpointError::CapacityExceeded);
        }
        self.entries
            .lock()
            .unwrap()
            .get(&reference.key)
            .filter(|(r, _)| *r == reference)
            .map(|(_, p)| p.clone())
            .ok_or(CheckpointError::NotFound)
    }
    fn delete(&self, reference: CheckpointRef) -> Result<(), CheckpointError> {
        self.deletes.fetch_add(1, Ordering::AcqRel);
        if self.fail_delete.load(Ordering::Acquire) {
            return Err(CheckpointError::Unavailable);
        }
        self.entries.lock().unwrap().remove(&reference.key);
        Ok(())
    }
    fn capacity(&self) -> CheckpointCapacity {
        CheckpointCapacity {
            limit_bytes: usize::MAX,
            committed_bytes: self
                .entries
                .lock()
                .unwrap()
                .values()
                .map(|(r, _)| r.bytes)
                .sum(),
            inflight_bytes: 0,
            abandoned_bytes: 0,
        }
    }
}
#[derive(Default)]
pub struct Protector {
    pub fail: AtomicBool,
    pub compressed_bound: AtomicUsize,
}
impl ICheckpointProtector for Protector {
    fn protected_size_limit(&self, plaintext: usize) -> Result<usize, CheckpointError> {
        let compressed = self.compressed_bound.load(Ordering::Acquire);
        if compressed > 0 {
            return Ok(compressed);
        }
        plaintext
            .checked_add(40)
            .ok_or(CheckpointError::CapacityExceeded)
    }
    fn protect(
        &self,
        key: CheckpointKey,
        checkpoint: TerminalCheckpoint,
    ) -> Result<ProtectedCheckpoint, CheckpointError> {
        if self.fail.load(Ordering::Acquire) {
            return Err(CheckpointError::AuthenticationFailed);
        }
        // Deliberately noncryptographic test encoding; verifies orchestration only.
        let mut bytes = checkpoint
            .bytes
            .iter()
            .map(|b| b ^ 0xa5)
            .collect::<Vec<_>>();
        bytes.extend([0xcc; 40]);
        Ok(ProtectedCheckpoint::new(
            key,
            checkpoint.descriptor.clone(),
            bytes,
        ))
    }
    fn open(
        &self,
        key: CheckpointKey,
        descriptor: &CheckpointDescriptor,
        checkpoint: ProtectedCheckpoint,
    ) -> Result<TerminalCheckpoint, CheckpointError> {
        if self.fail.load(Ordering::Acquire)
            || key != checkpoint.key
            || descriptor != &checkpoint.descriptor
        {
            return Err(CheckpointError::AuthenticationFailed);
        }
        let bytes = checkpoint.ciphertext();
        Ok(TerminalCheckpoint {
            descriptor: descriptor.clone(),
            bytes: bytes[..bytes.len() - 40].iter().map(|b| b ^ 0xa5).collect(),
        })
    }
}
