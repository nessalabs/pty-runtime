use super::support::*;
use crate::{
    process::OutputAcceptance,
    projection::{ProjectionError, ProjectionLimits, Residency},
};
use std::{
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
#[test]
fn independent_parser_admission_is_all_or_none_and_early_capacity_notice_survives() {
    let mut options = options();
    options.staging_bytes = 16;
    options.staging_slots = 1;
    let h = Harness::new(options, ProjectionLimits::default());
    assert_eq!(
        h.owner.stage_output(b"0123456789abcdef"),
        OutputAcceptance::Accepted
    );
    assert_eq!(h.owner.stage_output(b"x"), OutputAcceptance::Backpressure);
    assert_eq!(h.owner.status().published.offset, 16);
    h.step();
    let now = Instant::now();
    h.owner.wait_for_capacity(now + Duration::from_secs(5));
    assert!(now.elapsed() < Duration::from_secs(1));
    assert_eq!(h.owner.stage_output(b"x"), OutputAcceptance::Accepted);
    h.pump();
    assert_eq!(h.owner.status().processed.offset, 17);
    h.close();
}
#[test]
fn generated_reply_waits_for_single_writer_admission_without_refeeding_or_duplicate_send() {
    let h = Harness::standard();
    h.process.reject.store(true, Ordering::Release);
    h.owner.stage_output(b"?");
    h.step();
    for _ in 0..4 {
        h.step();
    }
    assert_eq!(h.owner.status().processed.offset, 1);
    assert!(h.process.writes.lock().unwrap().is_empty());
    h.owner.stage_output(b"next");
    h.step();
    assert_eq!(h.owner.status().processed.offset, 1);
    h.process.reject.store(false, Ordering::Release);
    h.pump();
    assert_eq!(*h.process.writes.lock().unwrap(), vec![b"R".to_vec()]);
    assert_eq!(h.owner.status().processed.offset, 5);
    h.close();
}
#[test]
fn partial_generated_reply_fails_projection_and_is_never_resent() {
    let h = Harness::standard();
    h.process.partial.store(true, Ordering::Release);
    h.owner.stage_output(b"?");
    h.pump();
    assert_eq!(h.owner.status().residency, Residency::Failed);
    assert_eq!(h.process.writes.lock().unwrap().len(), 1);
    for _ in 0..4 {
        h.step();
    }
    assert_eq!(h.process.writes.lock().unwrap().len(), 1);
    h.close();
}
#[test]
fn copied_views_retain_global_memory_admission_until_consumer_drop() {
    let opts = options();
    let limits = ProjectionLimits {
        view_bytes: opts.view_reservation().unwrap(),
        ..ProjectionLimits::default()
    };
    let h = Harness::new(opts, limits);
    let mut first = h.owner.view().unwrap();
    h.pump();
    let view = result(&mut first).unwrap();
    let mut second = h.owner.view().unwrap();
    h.pump();
    assert!(matches!(
        result(&mut second),
        Err(ProjectionError::Capacity)
    ));
    drop(view);
    let mut third = h.owner.view().unwrap();
    h.pump();
    assert!(result(&mut third).is_ok());
    h.close();
}
#[test]
fn native_feed_panic_preserves_unprocessed_staging_and_rejects_projection_operations() {
    let h = Harness::standard();
    h.probe.panic_feed.store(true, Ordering::Release);
    h.owner.stage_output(b"unapplied");
    h.pump();
    assert_eq!(h.owner.status().failure, Some(ProjectionError::Worker));
    assert_eq!(h.owner.status().processed.offset, 0);
    assert_eq!(h.owner.status().published.offset, 9);
    assert_eq!(h.owner.queue.queued(), 1);
    assert!(matches!(h.owner.view(), Err(ProjectionError::Worker)));
    h.close();
}

#[test]
fn global_parser_capacity_released_by_another_session_wakes_waiter() {
    use crate::{
        projection::ProjectionCoordinator, runtime::quota::Quota, scheduling::IScheduledWork,
    };
    use pty_runtime_domain::SessionLifetime;
    use std::sync::Arc;
    let limits = ProjectionLimits {
        staging_bytes: 16,
        ..ProjectionLimits::default()
    };
    let h = Harness::new(options(), limits);
    let other = ProjectionCoordinator::create(
        SessionLifetime::new(9, 2),
        options(),
        h.services.clone(),
        h.budgets.clone(),
        Arc::new(Quota::new(128)),
        Arc::new(Quota::new(8)),
    )
    .unwrap();
    assert_eq!(
        h.owner.stage_output(b"0123456789abcdef"),
        OutputAcceptance::Accepted
    );
    assert_eq!(other.stage_output(b"x"), OutputAcceptance::Backpressure);
    h.step();
    let now = Instant::now();
    other.wait_for_capacity(now + Duration::from_secs(5));
    assert!(now.elapsed() < Duration::from_secs(1));
    assert_eq!(other.stage_output(b"x"), OutputAcceptance::Accepted);
    other.run();
    assert_eq!(other.status().processed.offset, 1);
    let mut wait = other.close().unwrap();
    other.run();
    result(&mut wait).unwrap();
    h.close();
}

#[test]
fn resident_checkpoint_pin_reserves_plaintext_even_when_protection_bound_is_smaller() {
    let protector = std::sync::Arc::new(Protector::default());
    // This route must not invoke protection; model a provider advertising compression.
    protector.compressed_bound.store(1, Ordering::Release);
    protector.fail.store(true, Ordering::Release);
    let h = Harness::with_protector(
        options(),
        ProjectionLimits {
            checkpoint_bytes: 1025,
            ..ProjectionLimits::default()
        },
        protector,
    );
    let mut first = h.owner.checkpoint().unwrap();
    h.pump();
    let pin = result(&mut first).unwrap();
    let mut second = h.owner.checkpoint().unwrap();
    h.pump();
    assert!(matches!(
        result(&mut second),
        Err(ProjectionError::Capacity)
    ));
    drop(pin);
    let mut third = h.owner.checkpoint().unwrap();
    h.pump();
    assert!(result(&mut third).is_ok());
    h.close();
}
