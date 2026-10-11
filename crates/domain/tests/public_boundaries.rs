//! Independent public-domain boundary tests discovered in the coverage inventory.
use pty_runtime_domain::{
    ReplayBuffer, ReplayError, ReplayPage, SessionLifetime,
    checkpoint::{CheckpointCleanupLimits, CheckpointError},
    process::{CommandSpec, EnvironmentPolicy, ProcessError},
    projection::{
        ProjectionError, ProjectionLimits, ProjectionOptions, ProjectionPolicy, Residency,
    },
    session::{RuntimeError, RuntimeOptions, SessionOptions},
    terminal::{TerminalConfig, TerminalEffects, TerminalSize},
};
use std::{ffi::OsString, time::Duration};

fn command() -> CommandSpec {
    CommandSpec::new("/bin/tool".into(), "/".into(), vec![]).unwrap()
}

#[test]
fn cleanup_rejects_each_unbounded_work_dimension_before_use() {
    for (namespaces, entries, bytes) in [
        (0, 1, 1),
        (65537, 1, 1),
        (1, 0, 1),
        (1, 1_000_001, 1),
        (1, 1, 0),
        (1, 1, (1_u64 << 50) + 1),
    ] {
        assert_eq!(
            CheckpointCleanupLimits {
                namespaces,
                entries,
                bytes
            }
            .validate(),
            Err(CheckpointError::InvalidConfiguration)
        );
    }
    assert_eq!(
        CheckpointCleanupLimits {
            namespaces: 65536,
            entries: 1_000_000,
            bytes: 1_u64 << 50
        }
        .validate(),
        Ok(())
    );
}

#[test]
fn environment_limits_reject_oversized_collections_names_and_values() {
    let valid = command()
        .with_environment(
            EnvironmentPolicy::Inherit,
            vec![OsString::from("OLD")],
            vec![("NEW".into(), "value".into())],
        )
        .unwrap();
    assert_eq!(
        valid.removals().collect::<Vec<_>>(),
        [std::ffi::OsStr::new("OLD")]
    );
    assert_eq!(
        valid.overrides().collect::<Vec<_>>(),
        [(std::ffi::OsStr::new("NEW"), std::ffi::OsStr::new("value"))]
    );
    for removals in [
        vec![OsString::from("A"); 129],
        vec![OsString::from("a".repeat(8193))],
    ] {
        assert!(matches!(
            command().with_environment(EnvironmentPolicy::Empty, removals, vec![]),
            Err(ProcessError::InvalidCommand)
        ));
    }
    for overrides in [
        vec![(OsString::from("A"), OsString::from("B")); 129],
        vec![("A".into(), "x".repeat(8193).into())],
        vec![("A".into(), "x\0y".into())],
    ] {
        assert!(matches!(
            command().with_environment(EnvironmentPolicy::Empty, vec![], overrides),
            Err(ProcessError::InvalidCommand)
        ));
    }
    let large = vec![("A".into(), "x".repeat(8192).into()); 8];
    assert!(matches!(
        command().with_environment(EnvironmentPolicy::Empty, vec![], large),
        Err(ProcessError::InvalidCommand)
    ));
}

#[test]
fn public_option_validation_propagates_process_and_projection_failures() {
    let runtime = RuntimeOptions {
        output_page_bytes: 65537,
        ..RuntimeOptions::default()
    };
    assert_eq!(runtime.validate(), Err(RuntimeError::Capacity));
    let runtime = RuntimeOptions {
        projection: ProjectionLimits {
            view_bytes: 0,
            ..ProjectionLimits::default()
        },
        ..RuntimeOptions::default()
    };
    assert_eq!(
        runtime.validate(),
        Err(RuntimeError::Projection(
            ProjectionError::InvalidConfiguration
        ))
    );
    let limits = ProjectionLimits {
        stored_slots: 65537,
        ..ProjectionLimits::default()
    };
    assert_eq!(
        limits.validate().unwrap_err(),
        ProjectionError::InvalidConfiguration
    );
    let size = TerminalSize::new(80, 24).unwrap();
    let mut local = SessionOptions::raw(size);
    local.process.read_chunk = 0;
    assert_eq!(
        local.validate(&RuntimeOptions::default()),
        Err(RuntimeError::Process(ProcessError::Capacity))
    );
    let mut projection = ProjectionOptions::new(TerminalConfig::new(size));
    projection.terminal.reply_bytes = 0;
    assert_eq!(
        projection.validate().unwrap_err(),
        ProjectionError::Terminal(
            pty_runtime_domain::terminal::TerminalError::InvalidConfiguration
        )
    );
}

#[test]
fn allocation_refusal_preserves_replay_bytes_offsets_and_capacity() {
    let mut replay = ReplayBuffer::new(SessionLifetime::new(75, 1), usize::MAX);
    replay.append(b"retained-private-output").unwrap();
    let cursor = replay.floor();
    let before = replay.read(cursor, 64).unwrap();
    let end = replay.end();
    let allocated = replay.allocated_bytes();
    // VecDeque cannot represent a byte allocation exceeding isize::MAX. This
    // exercises deterministic capacity refusal without exhausting host memory.
    assert_eq!(
        replay.reserve_capacity(usize::MAX),
        Err(ReplayError::AllocationFailed)
    );
    assert_eq!(replay.read(cursor, 64).unwrap(), before);
    assert_eq!(replay.end(), end);
    assert_eq!(replay.allocated_bytes(), allocated);
    assert_eq!(replay.reserve_capacity(1).unwrap(), allocated);
}

#[test]
fn diagnostics_of_terminal_effects_and_replay_gaps_do_not_include_payload() {
    let payload = b"private-marker";
    let effects = TerminalEffects(payload.to_vec());
    assert_eq!(format!("{effects:?}"), "TerminalEffects { bytes: 14 }");
    let mut replay = ReplayBuffer::new(SessionLifetime::new(76, 1), 3);
    let cursor = replay.end();
    replay.append(payload).unwrap();
    let gap = replay.read(cursor, 8).unwrap();
    assert!(matches!(gap, ReplayPage::Gap { .. }));
    let debug = format!("{gap:?}");
    assert!(debug.starts_with("Gap") && !debug.contains("private-marker"));
    assert_eq!(
        format!("{:?}", replay.read(replay.end(), 1).unwrap()),
        "Pending"
    );
    assert_eq!(
        ProcessError::PermissionDenied.to_string(),
        "PermissionDenied"
    );
    assert_eq!(
        RuntimeError::Process(ProcessError::Io).to_string(),
        "Process(Io)"
    );
}

#[test]
fn closing_invalidates_activity_and_park_commit_without_reopening_the_policy() {
    let settings = ProjectionOptions::new(TerminalConfig::new(TerminalSize::new(2, 1).unwrap()));
    let mut policy =
        ProjectionPolicy::new(SessionLifetime::new(77, 1), settings, Duration::ZERO).unwrap();
    assert_eq!(
        policy.begin_park(Duration::ZERO),
        Err(ProjectionError::InvalidConfiguration)
    );
    policy.admit_output(1, Duration::ZERO).unwrap();
    assert_eq!(
        policy.begin_park(Duration::from_secs(60)),
        Err(ProjectionError::InvalidConfiguration)
    );
    policy.record_processed(1).unwrap();
    let attempt = policy.begin_park(Duration::from_secs(60)).unwrap();
    policy.close();
    assert_eq!(
        policy.record_activity(Duration::from_secs(61)),
        Err(ProjectionError::Closed)
    );
    assert!(!policy.commit_park(attempt, true));
    assert_eq!(policy.status().residency, Residency::Closing);
    policy.fail(ProjectionError::Worker);
    assert_eq!(policy.status().residency, Residency::Closing);
    policy.mark_closed();
    policy.close();
    assert_eq!(policy.status().residency, Residency::Closed);
}

#[cfg(target_pointer_width = "64")]
#[test]
fn byte_counter_exhaustion_preserves_admitted_and_processed_positions() {
    let settings = ProjectionOptions::new(TerminalConfig::new(TerminalSize::new(2, 1).unwrap()));
    let mut policy =
        ProjectionPolicy::new(SessionLifetime::new(78, 1), settings, Duration::ZERO).unwrap();
    policy.admit_output(usize::MAX, Duration::ZERO).unwrap();
    let admitted = policy.status();
    assert_eq!(
        policy.admit_output(1, Duration::ZERO),
        Err(ProjectionError::Capacity)
    );
    assert_eq!(policy.status(), admitted);
    policy.record_processed(usize::MAX).unwrap();
    let processed = policy.status();
    assert_eq!(policy.record_processed(1), Err(ProjectionError::Capacity));
    assert_eq!(policy.status(), processed);
}

#[test]
fn cumulative_skipped_history_overflow_preserves_the_current_restore_boundary() {
    use pty_runtime_domain::terminal::RestorationProgress;
    let settings = ProjectionOptions::new(TerminalConfig::new(TerminalSize::new(2, 1).unwrap()));
    let mut policy =
        ProjectionPolicy::new(SessionLifetime::new(79, 1), settings, Duration::ZERO).unwrap();
    let first = policy.begin_park(Duration::from_secs(60)).unwrap();
    assert!(policy.commit_park(first, true));
    policy.begin_restore().unwrap();
    policy
        .restoration_progress(RestorationProgress::FinishedWithSkippedHistory {
            skipped_pages: u64::MAX,
        })
        .unwrap();
    let second = policy.begin_park(Duration::from_secs(120)).unwrap();
    assert!(policy.commit_park(second, true));
    policy.begin_restore().unwrap();
    let before = policy.status();
    assert_eq!(
        policy.restoration_progress(RestorationProgress::UsableWithSkippedHistory {
            skipped_pages: 1,
        }),
        Err(ProjectionError::Capacity)
    );
    assert_eq!(policy.status(), before);
    policy
        .restoration_progress(RestorationProgress::Complete)
        .unwrap();
    assert_eq!(policy.status().residency, Residency::Resident);
    assert_eq!(policy.status().skipped_history_pages, u64::MAX);
}

#[test]
fn late_parking_failure_does_not_reopen_a_closed_lifetime() {
    let settings = ProjectionOptions::new(TerminalConfig::new(TerminalSize::new(2, 1).unwrap()));
    let mut policy =
        ProjectionPolicy::new(SessionLifetime::new(80, 1), settings, Duration::ZERO).unwrap();
    let pending = policy.begin_park(Duration::from_secs(60)).unwrap();
    policy.close();
    policy.mark_closed();
    policy.park_failed(ProjectionError::Worker, Duration::from_secs(61));
    assert_eq!(policy.status().residency, Residency::Closed);
    assert_eq!(
        policy.status().parking_failure,
        Some(ProjectionError::Worker)
    );
    assert_eq!(policy.park_delay(Duration::from_secs(120)), None);
    assert!(!policy.commit_park(pending, true));
}
