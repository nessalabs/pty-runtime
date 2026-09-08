//! Disk and injected provider contract, corruption, and privacy evidence.
use pty_runtime_application::checkpoint::{ICheckpointProtector, ICheckpointStore};
use pty_runtime_domain::{ReplayCursor, SessionLifetime, checkpoint::*, terminal::*};
use pty_runtime_infrastructure::checkpoint::{CheckpointProtector, FileCheckpointStore};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
fn path() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "pty-store-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
fn sample(generation: u64) -> (CheckpointKey, TerminalCheckpoint) {
    let lifetime = SessionLifetime::new(52, 8);
    (
        CheckpointKey {
            lifetime,
            generation,
        },
        TerminalCheckpoint {
            descriptor: CheckpointDescriptor {
                compatibility: "test-engine-v1".into(),
                processed: ReplayCursor {
                    lifetime,
                    offset: 500,
                },
                control_generation: 3,
            },
            bytes: b"private terminal state".to_vec(),
        },
    )
}
fn contract(store: &dyn ICheckpointStore) {
    let protector = CheckpointProtector::new(1024).unwrap();
    let (key, plain) = sample(1);
    let descriptor = plain.descriptor.clone();
    let encrypted = protector.protect(key, plain).unwrap();
    assert!(!encrypted.ciphertext().windows(8).any(|w| w == b"terminal"));
    let reference = store.commit(&encrypted).unwrap();
    assert_eq!(store.capacity().committed_bytes, reference.bytes);
    assert_eq!(
        store.commit(&encrypted),
        Err(CheckpointError::AlreadyExists)
    );
    assert_eq!(
        store.read(reference, reference.bytes - 1).unwrap_err(),
        CheckpointError::CapacityExceeded
    );
    let restored = protector
        .open(key, &descriptor, store.read(reference, 1024).unwrap())
        .unwrap();
    assert_eq!(restored.bytes, b"private terminal state");
    let mut wrong = reference;
    wrong.object_id[0] ^= 1;
    assert_eq!(
        store.read(wrong, 1024).unwrap_err(),
        CheckpointError::NotFound
    );
    assert_eq!(store.delete(wrong), Err(CheckpointError::NotFound));
    store.delete(reference).unwrap();
    store.delete(reference).unwrap();
    assert_eq!(store.capacity().committed_bytes, 0);
    assert_eq!(
        store.read(reference, 1024).unwrap_err(),
        CheckpointError::NotFound
    );
    let replacement = store.commit(&encrypted).unwrap();
    assert_ne!(replacement, reference);
    assert_eq!(
        store.read(reference, 1024).unwrap_err(),
        CheckpointError::NotFound
    );
    store.delete(replacement).unwrap();
}
#[test]
fn real_disk_contract_private_permissions_and_cleanup() {
    use std::os::unix::fs::PermissionsExt;
    let path = path();
    let store = FileCheckpointStore::new(&path, 1024).unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o700
    );
    contract(&store);
    let (key, plain) = sample(5);
    let encrypted = CheckpointProtector::new(1024)
        .unwrap()
        .protect(key, plain)
        .unwrap();
    store.commit(&encrypted).unwrap();
    let entries: Vec<_> = std::fs::read_dir(&path).unwrap().collect();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0]
            .as_ref()
            .unwrap()
            .metadata()
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    drop(store);
    assert!(!path.exists());
}
#[test]
fn ciphertext_corruption_truncation_wrong_key_and_metadata_replay_rejected() {
    let protector = CheckpointProtector::new(1024).unwrap();
    let (key, plain) = sample(1);
    let descriptor = plain.descriptor.clone();
    let encrypted = protector.protect(key, plain).unwrap();
    let other = CheckpointProtector::new(1024).unwrap();
    assert_eq!(
        other.open(key, &descriptor, encrypted.clone()).unwrap_err(),
        CheckpointError::AuthenticationFailed
    );
    for index in [0, 24, encrypted.ciphertext().len() - 1] {
        let mut bytes = encrypted.ciphertext().to_vec();
        bytes[index] ^= 1;
        let corrupt = ProtectedCheckpoint::new(key, descriptor.clone(), bytes);
        assert_eq!(
            protector.open(key, &descriptor, corrupt).unwrap_err(),
            CheckpointError::AuthenticationFailed
        );
    }
    for len in [0, 23, 39, encrypted.ciphertext().len() - 1] {
        let corrupt = ProtectedCheckpoint::new(
            key,
            descriptor.clone(),
            encrypted.ciphertext()[..len].to_vec(),
        );
        assert_eq!(
            protector.open(key, &descriptor, corrupt).unwrap_err(),
            CheckpointError::AuthenticationFailed
        );
    }
    for field in 0..5 {
        let mut changed_key = key;
        let mut changed = descriptor.clone();
        match field {
            0 => changed_key.generation += 1,
            1 => {
                changed_key.lifetime = SessionLifetime::new(52, 9);
                changed.processed.lifetime = changed_key.lifetime;
            }
            2 => changed.processed.offset += 1,
            3 => changed.control_generation += 1,
            _ => changed.compatibility.push('2'),
        }
        let forged = ProtectedCheckpoint::new(
            changed_key,
            changed.clone(),
            encrypted.ciphertext().to_vec(),
        );
        assert_eq!(
            protector.open(changed_key, &changed, forged).unwrap_err(),
            CheckpointError::AuthenticationFailed
        );
    }
}
#[test]
fn real_disk_quota_short_reads_symlinks_and_corrupt_files() {
    let path = path();
    let store = FileCheckpointStore::new(&path, 128).unwrap();
    let protector = CheckpointProtector::new(1024).unwrap();
    let (key, plain) = sample(1);
    let descriptor = plain.descriptor.clone();
    let reference = store
        .commit(&protector.protect(key, plain).unwrap())
        .unwrap();
    let (key2, mut large) = sample(2);
    large.bytes = vec![0; 128];
    assert_eq!(
        store.commit(&protector.protect(key2, large).unwrap()),
        Err(CheckpointError::CapacityExceeded)
    );
    assert_eq!(std::fs::read_dir(&path).unwrap().count(), 1);
    let file = std::fs::read_dir(&path)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let mut data = std::fs::read(&file).unwrap();
    data[30] ^= 1;
    std::fs::write(&file, &data).unwrap();
    assert_eq!(
        protector
            .open(key, &descriptor, store.read(reference, 128).unwrap())
            .unwrap_err(),
        CheckpointError::AuthenticationFailed
    );
    std::fs::write(&file, &data[..30]).unwrap();
    assert_eq!(
        store.read(reference, 128).unwrap_err(),
        CheckpointError::Unavailable
    );
    std::fs::remove_file(&file).unwrap();
    std::os::unix::fs::symlink("/dev/zero", &file).unwrap();
    assert!(store.read(reference, 128).is_err());
    store.delete(reference).unwrap();
    drop(store);
    assert!(!path.exists());
}
#[test]
fn existing_or_symlink_root_rejected_without_cleanup_of_foreign_content() {
    let path = path();
    std::fs::create_dir(&path).unwrap();
    std::fs::write(path.join("foreign"), b"retain").unwrap();
    assert!(FileCheckpointStore::new(&path, 1024).is_err());
    assert_eq!(std::fs::read(path.join("foreign")).unwrap(), b"retain");
    std::fs::remove_dir_all(&path).unwrap();
    std::os::unix::fs::symlink(std::env::temp_dir(), &path).unwrap();
    assert!(FileCheckpointStore::new(&path, 1024).is_err());
    std::fs::remove_file(&path).unwrap();
}
#[derive(Default)]
struct MemoryProvider(Mutex<HashMap<CheckpointKey, (CheckpointRef, ProtectedCheckpoint)>>);
impl ICheckpointStore for MemoryProvider {
    fn commit(&self, p: &ProtectedCheckpoint) -> Result<CheckpointRef, CheckpointError> {
        let mut state = self.0.lock().unwrap();
        if state.contains_key(&p.key) {
            return Err(CheckpointError::AlreadyExists);
        }
        let total: usize = state.values().map(|(r, _)| r.bytes).sum();
        if p.ciphertext().len() > 1024 - total {
            return Err(CheckpointError::CapacityExceeded);
        }
        let mut object_id = [0; 16];
        getrandom::getrandom(&mut object_id).unwrap();
        let r = CheckpointRef {
            key: p.key,
            bytes: p.ciphertext().len(),
            object_id,
        };
        state.insert(p.key, (r, p.clone()));
        Ok(r)
    }
    fn read(&self, r: CheckpointRef, max: usize) -> Result<ProtectedCheckpoint, CheckpointError> {
        if r.bytes > max {
            return Err(CheckpointError::CapacityExceeded);
        }
        self.0
            .lock()
            .unwrap()
            .get(&r.key)
            .filter(|(found, _)| *found == r)
            .map(|(_, p)| p.clone())
            .ok_or(CheckpointError::NotFound)
    }
    fn delete(&self, r: CheckpointRef) -> Result<(), CheckpointError> {
        let mut state = self.0.lock().unwrap();
        if let Some((found, _)) = state.get(&r.key) {
            if *found != r {
                return Err(CheckpointError::NotFound);
            }
        }
        state.remove(&r.key);
        Ok(())
    }
    fn capacity(&self) -> CheckpointCapacity {
        CheckpointCapacity {
            limit_bytes: 1024,
            committed_bytes: self.0.lock().unwrap().values().map(|(r, _)| r.bytes).sum(),
            inflight_bytes: 0,
            abandoned_bytes: 0,
        }
    }
}
#[test]
fn injected_provider_same_contract() {
    contract(&MemoryProvider::default());
}
