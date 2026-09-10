//! Independent ciphertext nonce and filesystem namespace ownership regressions.
use pty_runtime_application::checkpoint::{ICheckpointProtector, ICheckpointStore};
use pty_runtime_domain::{ReplayCursor, SessionLifetime, checkpoint::*, terminal::*};
use pty_runtime_infrastructure::checkpoint::{CheckpointProtector, FileCheckpointStore};
use std::collections::HashSet;

fn fixture() -> (CheckpointKey, TerminalCheckpoint) {
    let lifetime = SessionLifetime::new(83, 1);
    (
        CheckpointKey {
            lifetime,
            generation: 2,
        },
        TerminalCheckpoint {
            descriptor: CheckpointDescriptor {
                compatibility: CompatibilityId::new("adversarial-engine").unwrap(),
                processed: ReplayCursor {
                    lifetime,
                    offset: 99,
                },
                control_generation: 4,
            },
            bytes: b"synthetic-checkpoint-secret".to_vec(),
        },
    )
}
fn path() -> std::path::PathBuf {
    let mut suffix = [0u8; 8];
    getrandom::getrandom(&mut suffix).unwrap();
    std::env::temp_dir().join(format!(
        "checkpoint-independent-{:016x}",
        u64::from_ne_bytes(suffix)
    ))
}

#[test]
fn repeated_protection_uses_distinct_nonces_and_remains_authentic() {
    let protector = CheckpointProtector::new(1024).unwrap();
    let mut nonces = HashSet::new();
    for _ in 0..64 {
        let (key, plain) = fixture();
        let expected = plain.bytes.clone();
        let descriptor = plain.descriptor.clone();
        let protected = protector.protect(key, plain).unwrap();
        let bytes = protected.ciphertext();
        assert!(nonces.insert(bytes[bytes.len() - 24..].to_vec()));
        assert_eq!(
            protector.open(key, &descriptor, protected).unwrap().bytes,
            expected
        );
    }
}

#[test]
fn drop_does_not_remove_foreign_replacement_directory() {
    let original = path();
    let moved = original.with_extension("moved");
    let store = FileCheckpointStore::new(&original, 1024).unwrap();
    let (key, plain) = fixture();
    let protected = CheckpointProtector::new(1024)
        .unwrap()
        .protect(key, plain)
        .unwrap();
    store.commit(&protected).unwrap();
    std::fs::rename(&original, &moved).unwrap();
    std::fs::create_dir(&original).unwrap();
    drop(store);
    let replacement_survived = original.is_dir();
    // Cleanup fixtures before asserting, including on the failing implementation.
    if replacement_survived {
        std::fs::remove_dir(&original).unwrap();
    }
    std::fs::remove_dir_all(&moved).unwrap();
    assert!(
        replacement_survived,
        "Drop removed a directory it did not create/own"
    );
}

#[test]
fn permission_or_hardlink_change_rejects_read_without_exposing_payload() {
    use std::os::unix::fs::PermissionsExt;
    let root = path();
    let sibling = root.with_extension("hardlink");
    let store = FileCheckpointStore::new(&root, 1024).unwrap();
    let (key, plain) = fixture();
    let protected = CheckpointProtector::new(1024)
        .unwrap()
        .protect(key, plain)
        .unwrap();
    let reference = store.commit(&protected).unwrap();
    let file = std::fs::read_dir(&root)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o640)).unwrap();
    assert_eq!(
        store.read(reference, 1024).unwrap_err(),
        CheckpointError::Unavailable
    );
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    std::fs::hard_link(&file, &sibling).unwrap();
    assert_eq!(
        store.read(reference, 1024).unwrap_err(),
        CheckpointError::Unavailable
    );
    std::fs::remove_file(sibling).unwrap();
    assert!(store.read(reference, 1024).is_ok());
}

#[test]
fn failed_namespace_construction_rolls_back_under_restrictive_umask() {
    const CHILD_PATH: &str = "PTY_CHECKPOINT_UMASK_CHILD_PATH";
    if let Some(root) = std::env::var_os(CHILD_PATH) {
        // SAFETY: this branch runs only in an isolated test subprocess with one
        // selected test. It cannot change the parent test process's global umask.
        let previous = unsafe { libc::umask(0o100) };
        let result = FileCheckpointStore::new(std::path::Path::new(&root), 1024);
        // SAFETY: restore the isolated process's saved mask before assertions.
        unsafe { libc::umask(previous) };
        assert!(result.is_err(), "non-private directory was accepted");
        assert!(
            !std::path::Path::new(&root).exists(),
            "failed constructor leaked its newly created namespace"
        );
        return;
    }
    let root = path();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "failed_namespace_construction_rolls_back_under_restrictive_umask",
            "--test-threads=1",
        ])
        .env(CHILD_PATH, &root)
        .output()
        .unwrap();
    let leaked = root.exists();
    if leaked {
        std::fs::remove_dir(&root).unwrap();
    }
    assert!(
        output.status.success(),
        "isolated constructor check failed: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!leaked);
}
