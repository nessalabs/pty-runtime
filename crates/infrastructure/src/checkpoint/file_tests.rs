use super::*;
use pty_runtime_domain::{ReplayCursor, SessionLifetime};
use std::path::PathBuf;
use std::sync::{Arc, Barrier};
fn fixture(n: u64) -> ProtectedCheckpoint {
    let lifetime = SessionLifetime::new(6, 1);
    ProtectedCheckpoint::new(
        CheckpointKey {
            lifetime,
            generation: n,
        },
        CheckpointDescriptor {
            compatibility: "fixture".into(),
            processed: ReplayCursor {
                lifetime,
                offset: 9,
            },
            control_generation: 1,
        },
        vec![3; 60],
    )
}
fn path() -> PathBuf {
    let mut bytes = [0; 8];
    getrandom::getrandom(&mut bytes).unwrap();
    std::env::temp_dir().join(format!(
        "checkpoint-fault-{:016x}",
        u64::from_ne_bytes(bytes)
    ))
}
#[test]
fn partial_failed_writes_full_disk_and_zero_writes_clean_temporary_state() {
    let path = path();
    let store = FileCheckpointStore::new(&path, 128).unwrap();
    for (errno, expected) in [
        (libc::ENOSPC, CheckpointError::CapacityExceeded),
        (libc::EIO, CheckpointError::Unavailable),
    ] {
        let result = store.commit_with(&fixture(1), |file, bytes| {
            file.write_all(&bytes[..12])?;
            Err(std::io::Error::from_raw_os_error(errno))
        });
        assert_eq!(result, Err(expected));
        assert_eq!(std::fs::read_dir(&path).unwrap().count(), 0);
        assert_eq!(store.capacity().committed_bytes, 0);
        assert_eq!(store.capacity().inflight_bytes, 0);
    }
    let result = store.commit_with(
        &fixture(1),
        |_, _| Err(std::io::ErrorKind::WriteZero.into()),
    );
    assert_eq!(result, Err(CheckpointError::Unavailable));
    assert_eq!(std::fs::read_dir(&path).unwrap().count(), 0);
    let r = store.commit(&fixture(1)).unwrap();
    store.delete(r).unwrap();
}
#[test]
fn concurrent_commit_reservation_cannot_exceed_quota() {
    let path = path();
    let store = Arc::new(FileCheckpointStore::new(&path, 100).unwrap());
    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let first = {
        let store = store.clone();
        let entered = entered.clone();
        let release = release.clone();
        std::thread::spawn(move || {
            store.commit_with(&fixture(1), |file, bytes| {
                file.write_all(&bytes[..10])?;
                entered.wait();
                release.wait();
                file.write_all(&bytes[10..])
            })
        })
    };
    entered.wait();
    let files: Vec<_> = std::fs::read_dir(&path)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].extension().unwrap(), "pending");
    let second = {
        let store = store.clone();
        std::thread::spawn(move || store.commit(&fixture(2)))
    };
    release.wait();
    let r = first.join().unwrap().unwrap();
    assert_eq!(
        second.join().unwrap(),
        Err(CheckpointError::CapacityExceeded)
    );
    assert_eq!(store.capacity().committed_bytes, 60);
    assert_eq!(store.capacity().inflight_bytes, 0);
    store.delete(r).unwrap();
}

#[test]
fn failed_orphan_cleanup_retains_a_distinct_quota_charge() {
    let path = path();
    let store = FileCheckpointStore::new(&path, 100).unwrap();
    let result = store.commit_with_cleanup(
        &fixture(1),
        |file, bytes| {
            file.write_all(&bytes[..10])?;
            Err(std::io::Error::from_raw_os_error(libc::ENOSPC))
        },
        |_| Err(CheckpointError::Unavailable),
    );
    assert_eq!(result, Err(CheckpointError::CapacityExceeded));
    let usage = store.capacity();
    assert_eq!(usage.committed_bytes, 0);
    assert_eq!(usage.inflight_bytes, 0);
    assert_eq!(usage.abandoned_bytes, 60);
    assert_eq!(std::fs::read_dir(&path).unwrap().count(), 1);
    assert_eq!(
        store.commit(&fixture(2)),
        Err(CheckpointError::CapacityExceeded)
    );
    drop(store);
    assert!(!path.exists());
}

#[test]
fn renamed_namespace_drop_does_not_remove_unrelated_replacement() {
    let path = path();
    let moved = path.with_extension("moved");
    let store = FileCheckpointStore::new(&path, 100).unwrap();
    store.commit(&fixture(1)).unwrap();
    std::fs::rename(&path, &moved).unwrap();
    std::fs::create_dir(&path).unwrap();
    drop(store);
    assert!(path.is_dir(), "replacement is unrelated and must remain");
    assert_eq!(
        std::fs::read_dir(&moved).unwrap().count(),
        0,
        "owned files clean through anchored descriptor"
    );
    std::fs::remove_dir(&path).unwrap();
    std::fs::remove_dir(&moved).unwrap();
}
