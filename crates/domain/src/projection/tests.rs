use super::*;
use crate::terminal::ControlGeneration;
use crate::{
    SessionLifetime,
    terminal::{TerminalConfig, TerminalSize},
};
use std::time::Duration;
fn options() -> ProjectionOptions {
    ProjectionOptions::new(TerminalConfig {
        size: TerminalSize::new(2, 1).unwrap(),
        history_bytes: 128,
        continuation_bytes: 128,
        reply_bytes: 32,
        checkpoint_bytes: 1024,
        native_bytes: 4096,
        view_bytes: 1024,
        feed_bytes: 16,
    })
}
#[test]
fn mutation_invalidates_parking_without_advancing_unprocessed_position() {
    let mut policy =
        ProjectionPolicy::new(SessionLifetime::new(1, 1), options(), Duration::ZERO).unwrap();
    policy.admit_output(3, Duration::ZERO).unwrap();
    policy.record_processed(3).unwrap();
    policy
        .record_control_applied(ControlGeneration::from_raw(1))
        .unwrap();
    let attempt = policy.begin_park(Duration::from_secs(60)).unwrap();
    policy.admit_output(2, Duration::from_secs(60)).unwrap();
    assert!(!policy.commit_park(attempt, true));
    assert_eq!(policy.status().residency, Residency::Resident);
    assert_eq!(policy.status().processed.offset, 3);
    assert_eq!(policy.status().published.offset, 5);
    assert_eq!(attempt.control_generation, ControlGeneration::from_raw(1));
}
#[test]
fn successful_parking_does_not_consume_failed_retry_budget() {
    let mut policy =
        ProjectionPolicy::new(SessionLifetime::new(1, 1), options(), Duration::ZERO).unwrap();
    for _ in 0..10 {
        let attempt = policy.begin_park(Duration::from_secs(60)).unwrap();
        assert!(policy.commit_park(attempt, true));
        policy.begin_restore().unwrap();
        let _ = policy.restoration_progress(crate::terminal::RestorationProgress::Complete);
    }
}
#[test]
fn failed_retries_are_bounded_until_real_mutation() {
    let mut policy =
        ProjectionPolicy::new(SessionLifetime::new(1, 1), options(), Duration::ZERO).unwrap();
    for seconds in [60, 65, 70] {
        policy.begin_park(Duration::from_secs(seconds)).unwrap();
        policy.park_failed(ProjectionError::Capacity, Duration::from_secs(seconds));
    }
    assert_eq!(policy.park_delay(Duration::from_secs(100)), None);
    policy.record_activity(Duration::from_secs(100)).unwrap();
    assert_eq!(
        policy.park_delay(Duration::from_secs(100)),
        Some(Duration::from_secs(60))
    );
}
#[test]
fn close_rejects_late_commit_and_usable_completion_cannot_revive() {
    let mut policy =
        ProjectionPolicy::new(SessionLifetime::new(1, 1), options(), Duration::ZERO).unwrap();
    let attempt = policy.begin_park(Duration::from_secs(60)).unwrap();
    policy.close();
    assert!(!policy.commit_park(attempt, true));
    assert_eq!(
        policy.restoration_progress(crate::terminal::RestorationProgress::Usable),
        Err(ProjectionError::Closed)
    );
    assert_eq!(policy.status().residency, Residency::Closing);
    policy.mark_closed();
    let _ = policy.restoration_progress(crate::terminal::RestorationProgress::Complete);
    assert_eq!(policy.status().residency, Residency::Closed);
    assert_eq!(
        policy.admit_output(1, Duration::from_secs(61)),
        Err(ProjectionError::Closed)
    );
}
#[test]
fn invalid_policy_and_out_of_order_processed_controls_are_rejected() {
    let mut invalid = options();
    invalid.park_after = Duration::ZERO;
    assert!(ProjectionPolicy::new(SessionLifetime::new(1, 1), invalid, Duration::ZERO).is_err());
    let mut policy =
        ProjectionPolicy::new(SessionLifetime::new(1, 1), options(), Duration::ZERO).unwrap();
    assert!(policy.record_processed(1).is_err());
    assert!(
        policy
            .record_control_applied(ControlGeneration::from_raw(2))
            .is_err()
    );
    assert_eq!(policy.status().processed.offset, 0);
}

#[test]
fn restoration_rejects_unadmitted_sources_and_preserves_failures() {
    use crate::terminal::RestorationProgress;
    let mut policy =
        ProjectionPolicy::new(SessionLifetime::new(1, 1), options(), Duration::ZERO).unwrap();
    assert!(policy.begin_restore().is_err());
    assert!(
        policy
            .restoration_progress(RestorationProgress::Complete)
            .is_err()
    );
    assert_eq!(policy.status().residency, Residency::Resident);
    let attempt = policy.begin_park(Duration::from_secs(60)).unwrap();
    assert!(policy.begin_restore().is_err());
    assert!(policy.commit_park(attempt, true));
    assert!(
        policy
            .restoration_progress(RestorationProgress::Usable)
            .is_err()
    );
    assert_eq!(policy.status().residency, Residency::Parked);
    policy.begin_restore().unwrap();
    assert!(policy.begin_restore().is_err());
    policy
        .restoration_progress(RestorationProgress::Usable)
        .unwrap();
    policy.fail(ProjectionError::Worker);
    assert_eq!(
        policy.restoration_progress(RestorationProgress::Complete),
        Err(ProjectionError::Worker)
    );
    assert_eq!(policy.status().residency, Residency::Failed);
}

#[test]
fn skipped_history_progress_cannot_regress_into_complete_history() {
    use crate::terminal::RestorationProgress;
    let mut policy =
        ProjectionPolicy::new(SessionLifetime::new(4, 1), options(), Duration::ZERO).unwrap();
    let attempt = policy.begin_park(Duration::from_secs(60)).unwrap();
    assert!(policy.commit_park(attempt, true));
    policy.begin_restore().unwrap();
    let skipped = RestorationProgress::UsableWithSkippedHistory { skipped_pages: 2 };
    policy.restoration_progress(skipped).unwrap();
    assert_eq!(policy.status().skipped_history_pages, 2);
    assert_eq!(
        policy.restoration_progress(RestorationProgress::Complete),
        Err(ProjectionError::InvalidConfiguration)
    );
    assert_eq!(policy.status().history, skipped);
    let finish = RestorationProgress::FinishedWithSkippedHistory { skipped_pages: 3 };
    policy.restoration_progress(finish).unwrap();
    assert_eq!(policy.status().history, finish);
    assert_eq!(policy.status().residency, Residency::Resident);
    assert_eq!(policy.status().skipped_history_pages, 3);
}
