//! Actual owner death, live-owner exclusion and bounded safe disk reclamation.
#[path = "fixtures/checkpoint_crash.rs"]
mod support;
use pty_runtime_application::checkpoint::ICheckpointStore;
use pty_runtime_domain::checkpoint::*;
use pty_runtime_infrastructure::checkpoint::FileCheckpointStore;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt, symlink};
use support::*;

#[test]
fn checkpoint_crash_fixture() {
    child();
}

#[test]
fn dead_owner_is_reclaimed_while_live_owner_and_its_ciphertext_survive() {
    let fixture = Fixture::new();
    let live = FileCheckpointStore::temporary(Some(&fixture.0), 4096).unwrap();
    let checkpoint = protected(9);
    let reference = live.commit(&checkpoint).unwrap();
    let mut child = ChildOwner::new(&fixture.0);
    let before = FileCheckpointStore::cleanup_abandoned(
        Some(&fixture.0),
        CheckpointCleanupLimits::default(),
    )
    .unwrap();
    assert_eq!(before.live_namespaces, 2);
    assert_eq!(before.reclaimed_objects, 0);
    child.crash();
    let next = FileCheckpointStore::temporary(Some(&fixture.0), 4096).unwrap();
    let report = next.cleanup_report();
    assert!(report.scan_complete);
    assert_eq!(report.live_namespaces, 1);
    assert_eq!(report.reclaimed_namespaces, 1);
    assert_eq!(report.reclaimed_objects, 2);
    assert_eq!(report.reclaimed_bytes, 120);
    assert_eq!(report.failure, None);
    assert_eq!(
        live.read(reference, 4096).unwrap().ciphertext(),
        checkpoint.ciphertext()
    );
    assert_eq!(live.capacity().committed_bytes, 60);
}

#[test]
fn cleanup_budget_reports_partial_work_and_blocks_known_abandoned_admission() {
    let fixture = Fixture::new();
    let mut child = ChildOwner::new(&fixture.0);
    child.crash();
    let small = CheckpointCleanupLimits {
        bytes: 30,
        ..Default::default()
    };
    assert!(matches!(
        FileCheckpointStore::temporary_with_cleanup(Some(&fixture.0), 4096, small),
        Err(CheckpointError::CapacityExceeded)
    ));
    let partial = FileCheckpointStore::cleanup_abandoned(
        Some(&fixture.0),
        CheckpointCleanupLimits {
            bytes: 60,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(partial.reclaimed_objects, 1);
    assert_eq!(partial.reclaimed_bytes, 60);
    assert!(partial.abandoned_incomplete);
    assert_eq!(partial.failure, None);
    let final_pass = FileCheckpointStore::cleanup_abandoned(
        Some(&fixture.0),
        CheckpointCleanupLimits::default(),
    )
    .unwrap();
    assert_eq!(final_pass.reclaimed_namespaces, 1);
    assert_eq!(final_pass.reclaimed_bytes, 60);
    assert!(!final_pass.abandoned_incomplete);
    assert_eq!(std::fs::read_dir(fixture.arena()).unwrap().count(), 0);
}

#[test]
fn live_prefix_cannot_hide_abandoned_data_and_admit_repeated_owners() {
    let fixture = Fixture::new();
    drop(FileCheckpointStore::temporary(Some(&fixture.0), 4096).unwrap());
    for index in 1..=3 {
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(fixture.arena().join(format!("owner-{index:032x}")))
            .unwrap();
    }
    // Use actual directory enumeration order so the locked prefix is portable.
    let paths: Vec<_> = std::fs::read_dir(fixture.arena())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    let owners: Vec<_> = paths[..2]
        .iter()
        .map(|path| {
            use std::os::fd::AsRawFd;
            let owner = std::fs::File::open(path).unwrap();
            // SAFETY: this test owns the live directory FD for the lock lifetime.
            assert_eq!(unsafe { libc::flock(owner.as_raw_fd(), libc::LOCK_EX) }, 0);
            owner
        })
        .collect();
    let hidden = paths[2].join("0000000000000001.checkpoint");
    std::fs::write(&hidden, [7; 60]).unwrap();
    std::fs::set_permissions(&hidden, std::fs::Permissions::from_mode(0o600)).unwrap();
    let tiny = CheckpointCleanupLimits {
        namespaces: 2,
        entries: 16,
        bytes: 4096,
    };
    for _ in 0..3 {
        let report = FileCheckpointStore::cleanup_abandoned(Some(&fixture.0), tiny).unwrap();
        assert_eq!(report.live_namespaces, 2);
        assert!(!report.scan_complete);
        assert!(!report.abandoned_incomplete);
        assert_eq!(report.failure, None);
        assert!(matches!(
            FileCheckpointStore::temporary_with_cleanup(Some(&fixture.0), 4096, tiny),
            Err(CheckpointError::CapacityExceeded)
        ));
        assert_eq!(std::fs::read_dir(fixture.arena()).unwrap().count(), 3);
        assert_eq!(std::fs::metadata(&hidden).unwrap().len(), 60);
    }
    let next = FileCheckpointStore::temporary(Some(&fixture.0), 4096).unwrap();
    assert_eq!(next.cleanup_report().reclaimed_bytes, 60);
    assert!(next.cleanup_report().scan_complete);
    assert!(!hidden.exists());
    drop((next, owners));
}

#[test]
fn symlinks_hardlinks_and_unrecognized_objects_are_never_deleted() {
    for mode in ["symlink", "hardlink", "unknown", "permissions"] {
        let fixture = Fixture::new();
        let mut child = ChildOwner::new(&fixture.0);
        child.crash();
        let namespace = fixture.namespace();
        let object = namespace.join("0000000000000001.checkpoint");
        let foreign = fixture.0.join("foreign");
        std::fs::write(&foreign, b"must-survive").unwrap();
        match mode {
            "symlink" => {
                std::fs::remove_file(&object).unwrap();
                symlink(&foreign, &object).unwrap();
            }
            "hardlink" => {
                std::fs::remove_file(&object).unwrap();
                std::fs::hard_link(&foreign, &object).unwrap();
                std::fs::set_permissions(&foreign, std::fs::Permissions::from_mode(0o600)).unwrap();
            }
            "unknown" => {
                std::fs::rename(&object, namespace.join("unrecognized")).unwrap();
            }
            _ => std::fs::set_permissions(&object, std::fs::Permissions::from_mode(0o640)).unwrap(),
        }
        let report = FileCheckpointStore::cleanup_abandoned(
            Some(&fixture.0),
            CheckpointCleanupLimits::default(),
        )
        .unwrap();
        assert!(report.abandoned_incomplete);
        assert_eq!(report.failure, Some(CheckpointError::Unavailable));
        assert!(namespace.exists());
        assert_eq!(std::fs::read(&foreign).unwrap(), b"must-survive");
        assert!(if mode == "unknown" {
            namespace.join("unrecognized").exists()
        } else {
            std::fs::symlink_metadata(&object).is_ok()
        });
    }
}

#[test]
fn interrupted_empty_construction_is_reclaimed_but_foreign_arena_entries_remain() {
    let fixture = Fixture::new();
    drop(FileCheckpointStore::temporary(Some(&fixture.0), 4096).unwrap());
    let empty = fixture.arena().join(format!("owner-{}", "a".repeat(32)));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&empty)
        .unwrap();
    let report = FileCheckpointStore::cleanup_abandoned(
        Some(&fixture.0),
        CheckpointCleanupLimits::default(),
    )
    .unwrap();
    assert_eq!(report.reclaimed_namespaces, 1);
    assert!(!empty.exists());
    let unrelated = fixture.arena().join("unrelated");
    std::fs::write(&unrelated, b"unrelated").unwrap();
    let report = FileCheckpointStore::cleanup_abandoned(
        Some(&fixture.0),
        CheckpointCleanupLimits::default(),
    )
    .unwrap();
    assert_eq!(report.examined_entries, 1);
    assert_eq!(report.failure, Some(CheckpointError::Unavailable));
    assert!(unrelated.exists());
}

#[test]
fn namespace_and_arena_symlinks_are_rejected_without_traversal() {
    let fixture = Fixture::new();
    drop(FileCheckpointStore::temporary(Some(&fixture.0), 4096).unwrap());
    let arena = fixture.arena();
    let foreign = fixture.0.join("foreign-directory");
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&foreign)
        .unwrap();
    std::fs::write(foreign.join("must-survive"), b"foreign").unwrap();
    let candidate = arena.join(format!("owner-{}", "b".repeat(32)));
    symlink(&foreign, &candidate).unwrap();
    let report = FileCheckpointStore::cleanup_abandoned(
        Some(&fixture.0),
        CheckpointCleanupLimits::default(),
    )
    .unwrap();
    assert_eq!(report.failure, Some(CheckpointError::Unavailable));
    assert!(std::fs::symlink_metadata(&candidate).unwrap().is_symlink());
    std::fs::remove_file(candidate).unwrap();
    std::fs::remove_dir(&arena).unwrap();
    symlink(&foreign, &arena).unwrap();
    assert!(
        FileCheckpointStore::cleanup_abandoned(
            Some(&fixture.0),
            CheckpointCleanupLimits::default()
        )
        .is_err()
    );
    assert_eq!(
        std::fs::read(foreign.join("must-survive")).unwrap(),
        b"foreign"
    );
}

#[test]
fn direct_namespace_initialization_cannot_race_a_held_maintenance_lock() {
    use std::os::fd::AsRawFd;
    let fixture = Fixture::new();
    // Start from an empty fixture arena: a prior production maintenance FD could
    // still be inherited by another test's concurrently forked pre-exec child.
    let arena = fixture.empty_arena();
    let lock = std::fs::File::open(&arena).unwrap();
    // SAFETY: this fixture owns a separate live directory descriptor. No name
    // replacement occurs; the held lock represents an in-progress maintenance pass.
    assert_eq!(
        unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );
    let candidate = arena.join(format!("owner-{}", "c".repeat(32)));
    let child_path = candidate.clone();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        sender
            .send(matches!(
                FileCheckpointStore::new(child_path, 4096),
                Err(CheckpointError::CapacityExceeded)
            ))
            .unwrap();
    });
    assert_eq!(
        receiver.recv_timeout(std::time::Duration::from_secs(3)),
        Ok(true)
    );
    assert!(
        !candidate.exists(),
        "initialization raced the maintenance lock"
    );
    drop(lock);
    let owner = FileCheckpointStore::new(&candidate, 4096).unwrap();
    let report = FileCheckpointStore::cleanup_abandoned(
        Some(&fixture.0),
        CheckpointCleanupLimits::default(),
    )
    .unwrap();
    assert_eq!(report.live_namespaces, 1);
    drop(owner);
}

#[test]
fn inherited_maintenance_lock_blocks_admission_until_fork_child_releases_it() {
    use std::os::fd::AsRawFd;
    const ISOLATED: &str = "PTY_CHECKPOINT_ISOLATED_FORK_LOCK";
    if std::env::var_os(ISOLATED).is_none() {
        // The intentionally non-exec child must not retain other parallel tests'
        // locks for its full one-second contention fixture.
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "inherited_maintenance_lock_blocks_admission_until_fork_child_releases_it",
                "--test-threads=1",
            ])
            .env(ISOLATED, "1")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stdout)
        );
        return;
    }
    let fixture = Fixture::new();
    let arena = fixture.empty_arena();
    let lock = std::fs::File::open(&arena).unwrap();
    // SAFETY: this descriptor is owned and stays live through the fork below.
    assert_eq!(unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) }, 0);
    let child = ForkLockHolder::new();
    drop(lock);
    let probe = std::fs::File::open(&arena).unwrap();
    // SAFETY: the independent probe descriptor is owned for this syscall.
    assert_eq!(
        unsafe { libc::flock(probe.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        -1
    );
    assert_eq!(
        std::io::Error::last_os_error().kind(),
        std::io::ErrorKind::WouldBlock
    );
    drop(probe);
    // Closing in the parent does not release a lock inherited before exec,
    // despite CLOEXEC. The production finite deadline must report contention.
    assert!(matches!(
        FileCheckpointStore::temporary(Some(&fixture.0), 4096),
        Err(CheckpointError::CapacityExceeded)
    ));
    assert_eq!(std::fs::read_dir(&arena).unwrap().count(), 0);
    drop(child);
    let owner = FileCheckpointStore::temporary(Some(&fixture.0), 4096).unwrap();
    assert!(owner.cleanup_report().scan_complete);
}
