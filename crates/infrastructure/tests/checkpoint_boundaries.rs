//! Public admission failures must preserve filesystem ownership and quota.
use pty_runtime_application::checkpoint::{ICheckpointProtector, ICheckpointStore};
use pty_runtime_domain::{ReplayCursor, SessionLifetime, checkpoint::*, terminal::*};
use pty_runtime_infrastructure::checkpoint::{CheckpointProtector, FileCheckpointStore};
use std::{fs, path::PathBuf};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let mut random = [0; 16];
        getrandom::getrandom(&mut random).unwrap();
        let path = std::env::temp_dir().join(format!(
            "checkpoint-boundaries-{:032x}",
            u128::from_ne_bytes(random)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn checkpoint(bytes: Vec<u8>) -> (CheckpointKey, TerminalCheckpoint) {
    let lifetime = SessionLifetime::new(84, 1);
    (
        CheckpointKey {
            lifetime,
            generation: 1,
        },
        TerminalCheckpoint {
            descriptor: CheckpointDescriptor {
                compatibility: CompatibilityId::new("boundary-fixture").unwrap(),
                processed: ReplayCursor {
                    lifetime,
                    offset: 1,
                },
                control_generation: ControlGeneration::from_raw(1),
            },
            bytes,
        },
    )
}

#[test]
fn protector_enforces_plaintext_limit_in_both_directions() {
    for invalid in [0, 512 * 1024 * 1024 + 1, usize::MAX] {
        assert!(matches!(
            CheckpointProtector::new(invalid),
            Err(CheckpointError::InvalidConfiguration)
        ));
    }
    let protector = CheckpointProtector::new(4).unwrap();
    assert_eq!(protector.protected_size_limit(4), Ok(44));
    for invalid in [5, usize::MAX] {
        assert_eq!(
            protector.protected_size_limit(invalid),
            Err(CheckpointError::CapacityExceeded)
        );
    }
    let (key, too_large) = checkpoint(vec![7; 5]);
    assert_eq!(
        protector.protect(key, too_large).unwrap_err(),
        CheckpointError::CapacityExceeded
    );
    let (key, plain) = checkpoint(vec![7; 4]);
    let descriptor = plain.descriptor.clone();
    let protected = protector.protect(key, plain).unwrap();
    assert_eq!(protected.ciphertext().len(), 44);
    let oversized = ProtectedCheckpoint::new(key, descriptor.clone(), vec![0; 45]);
    assert_eq!(
        protector.open(key, &descriptor, oversized).unwrap_err(),
        CheckpointError::CapacityExceeded
    );
    assert_eq!(
        protector.open(key, &descriptor, protected).unwrap().bytes,
        vec![7; 4]
    );
}

#[test]
fn protector_rejects_inconsistent_lifetime_and_mismatched_envelope() {
    let protector = CheckpointProtector::new(16).unwrap();
    let (key, mut plain) = checkpoint(vec![7; 4]);
    plain.descriptor.processed.lifetime = SessionLifetime::new(85, 1);
    let inconsistent = plain.descriptor.clone();
    assert_eq!(
        protector.protect(key, plain).unwrap_err(),
        CheckpointError::InvalidConfiguration
    );
    let envelope = ProtectedCheckpoint::new(key, inconsistent.clone(), vec![0; 44]);
    assert_eq!(
        protector.open(key, &inconsistent, envelope).unwrap_err(),
        CheckpointError::InvalidConfiguration
    );
    let (key, plain) = checkpoint(vec![7; 4]);
    let descriptor = plain.descriptor.clone();
    let protected = protector.protect(key, plain).unwrap();
    let mut wrong = key;
    wrong.generation += 1;
    assert_eq!(
        protector
            .open(wrong, &descriptor, protected.clone())
            .unwrap_err(),
        CheckpointError::AuthenticationFailed
    );
    assert_eq!(
        protector
            .open(key, &inconsistent, protected.clone())
            .unwrap_err(),
        CheckpointError::AuthenticationFailed
    );
    assert_eq!(
        protector.open(key, &descriptor, protected).unwrap().bytes,
        vec![7; 4]
    );
}

#[test]
fn invalid_store_configuration_creates_no_namespace() {
    let directory = Directory::new();
    assert!(matches!(
        FileCheckpointStore::new(directory.0.join("store"), 0),
        Err(CheckpointError::InvalidConfiguration)
    ));
    assert!(matches!(
        FileCheckpointStore::temporary(Some(&directory.0), 0),
        Err(CheckpointError::InvalidConfiguration)
    ));
    let invalid = CheckpointCleanupLimits {
        entries: 0,
        ..Default::default()
    };
    assert!(matches!(
        FileCheckpointStore::temporary_with_cleanup(Some(&directory.0), 16, invalid),
        Err(CheckpointError::InvalidConfiguration)
    ));
    assert_eq!(
        FileCheckpointStore::cleanup_abandoned(Some(&directory.0), invalid),
        Err(CheckpointError::InvalidConfiguration)
    );
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
}

#[test]
fn failed_cleanup_admission_preserves_unrecognized_content() {
    let directory = Directory::new();
    drop(FileCheckpointStore::temporary(Some(&directory.0), 16).unwrap());
    let arena = fs::read_dir(&directory.0)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let foreign = arena.join("foreign-object");
    fs::write(&foreign, b"keep this unrelated data").unwrap();
    assert!(matches!(
        FileCheckpointStore::temporary(Some(&directory.0), 16),
        Err(CheckpointError::Unavailable)
    ));
    assert_eq!(fs::read(&foreign).unwrap(), b"keep this unrelated data");
    assert_eq!(fs::read_dir(&arena).unwrap().count(), 1);
    fs::remove_file(foreign).unwrap();
    let store = FileCheckpointStore::temporary(Some(&directory.0), 16).unwrap();
    assert!(store.cleanup_report().scan_complete);
    assert_eq!(store.cleanup_report().failure, None);
}

#[test]
fn empty_commit_does_not_consume_quota_or_claim_its_key() {
    let directory = Directory::new();
    let root = directory.0.join("store");
    let store = FileCheckpointStore::new(&root, 4).unwrap();
    let (key, plain) = checkpoint(Vec::new());
    let empty = ProtectedCheckpoint::new(key, plain.descriptor.clone(), Vec::new());
    assert_eq!(
        store.commit(&empty),
        Err(CheckpointError::InvalidConfiguration)
    );
    assert_eq!(store.capacity().committed_bytes, 0);
    assert_eq!(store.capacity().inflight_bytes, 0);
    assert_eq!(store.capacity().abandoned_bytes, 0);
    assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
    let valid = ProtectedCheckpoint::new(key, plain.descriptor.clone(), vec![7; 4]);
    let reference = store.commit(&valid).unwrap();
    assert_eq!(store.read(reference, 4).unwrap().ciphertext(), &[7; 4]);
    assert_eq!(store.capacity().committed_bytes, 4);
    store.delete(reference).unwrap();
    assert_eq!(store.capacity().committed_bytes, 0);
}
