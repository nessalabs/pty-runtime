use super::support::*;
use crate::{
    process::OutputAcceptance,
    projection::{ProjectionError, ProjectionLimits, ProjectionOptions, Residency},
};
use pty_runtime_domain::terminal::{TerminalConfig, TerminalSize};
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

/// An empty chunk is accepted without being queued, so it must not wake the
/// worker or touch projection state.
///
/// `IProcessEvents::output` permits an empty slice, and `stage_output` is public
/// API. A projection whose scheduler registration has already been released --
/// the state after cleanup -- would otherwise turn a no-op chunk into a worker
/// failure, because waking a released handle reports `Worker`.
#[test]
fn empty_output_chunk_is_accepted_without_waking_or_failing() {
    let h = Harness::standard();
    h.owner.wiring.take_handle();
    assert_eq!(h.owner.stage_output(b""), OutputAcceptance::Accepted);
    assert!(h.owner.status().failure.is_none());
    assert_eq!(h.owner.queue.queued(), 0);
}

/// One worker run feeds many queued chunks, bounded, and stops for native work.
///
/// Everything around a chunk — the workspace lock, the in-flight poll, the phase
/// decision, the scheduler round trip — is paid per chunk rather than per byte.
/// Taking one chunk per run made that overhead the dominant cost for small
/// writes: Experiment 0005 measured `ProjectedOutput` p99 at 225-273 ms against
/// a 20 ms target with 64-byte chunks, while 64 KiB chunks passed.
///
/// The bound matters as much as the batching. A session with a deep queue must
/// not hold its workspace lock indefinitely against its own observers or a
/// close, so a run stops after `OUTPUT_BATCH` chunks even with more queued.
#[test]
fn one_run_feeds_a_bounded_batch_of_queued_output() {
    let mut options = options();
    options.staging_slots = 256;
    let h = Harness::new(options, ProjectionLimits::default());
    for _ in 0..40 {
        assert_eq!(h.owner.stage_output(b"ab"), OutputAcceptance::Accepted);
    }
    assert_eq!(h.owner.queue.queued(), 40);

    h.step();
    let after_one_run = h.owner.status().processed.offset;
    assert_eq!(
        after_one_run, 64,
        "one run should feed exactly the 32-chunk bound, two bytes each"
    );
    assert_eq!(h.owner.queue.queued(), 8, "the rest must stay queued");

    h.pump();
    assert_eq!(h.owner.status().processed.offset, 80);
    h.close();
}

/// The batch is bounded by bytes as well as by chunk count.
///
/// `feed_bytes` validates up to 1 MiB, so a 32-chunk bound alone would let one
/// run parse up to 32 MiB while holding a scheduler worker. With large chunks
/// the byte bound must bind first.
#[test]
fn a_large_chunk_batch_is_bounded_by_bytes_not_chunk_count() {
    let mut options = ProjectionOptions::new(TerminalConfig {
        feed_bytes: 64 * 1024,
        ..TerminalConfig::new(TerminalSize::new(80, 24).unwrap())
    });
    options.staging_slots = 256;
    let h = Harness::new(options, ProjectionLimits::default());
    let chunk = vec![b'a'; 64 * 1024];
    for _ in 0..10 {
        assert_eq!(h.owner.stage_output(&chunk), OutputAcceptance::Accepted);
    }

    h.step();
    assert_eq!(
        h.owner.queue.queued(),
        6,
        "256 KiB total, the run's own first chunk included, is four chunks - \
         well inside the 32-chunk bound, so bytes are what stopped it"
    );
    assert_eq!(h.owner.status().processed.offset, 4 * 64 * 1024);
    h.close();
}

/// At the maximum supported `feed_bytes`, one run feeds one chunk.
///
/// This is the case the byte bound exists for. A 1 MiB chunk exhausts the bound
/// on its own, so the run must not batch a second and do 2 MiB of parsing while
/// holding the workspace and a scheduler worker — which is twice what the
/// unbatched code ever did in one run.
#[test]
fn a_maximum_sized_chunk_is_fed_alone() {
    let mut options = ProjectionOptions::new(TerminalConfig {
        feed_bytes: 1024 * 1024,
        ..TerminalConfig::new(TerminalSize::new(80, 24).unwrap())
    });
    options.staging_bytes = 8 * 1024 * 1024;
    options.staging_slots = 256;
    let h = Harness::new(options, ProjectionLimits::default());
    let chunk = vec![b'a'; 1024 * 1024];
    for _ in 0..2 {
        assert_eq!(h.owner.stage_output(&chunk), OutputAcceptance::Accepted);
    }

    h.step();
    assert_eq!(
        h.owner.queue.queued(),
        1,
        "the first chunk alone exhausts the byte bound, so the second must wait \
         for its own run"
    );
    assert_eq!(h.owner.status().processed.offset, 1024 * 1024);
    h.close();
}

/// A failed projection ends the batch, and the output failure retained stays
/// retained.
///
/// An oversized generated reply fails the projection and then falls through to
/// an immediate schedule with no reply set, so nothing else in the loop
/// condition stops it. `queue.fail` keeps staged output on purpose; a batch that
/// kept running would dequeue exactly those bytes and drop them, because a
/// failed policy cannot process them and nothing requeues them.
#[test]
fn a_failed_projection_stops_the_output_batch_and_keeps_queued_output() {
    let h = Harness::standard();
    let bound = options().terminal.reply_bytes;
    h.probe.reply_size.store(bound + 1, Ordering::Release);
    // The first chunk fails the projection; the three behind it must survive.
    for _ in 0..4 {
        assert_eq!(h.owner.stage_output(b"?"), OutputAcceptance::Accepted);
    }
    assert_eq!(h.owner.queue.queued(), 4);

    h.step();
    assert_eq!(
        h.owner.status().failure,
        Some(ProjectionError::Capacity),
        "the oversized reply must fail the projection"
    );
    assert_eq!(
        h.owner.status().processed.offset,
        0,
        "no chunk was processed, so no position may advance"
    );
    assert_eq!(
        h.owner.queue.queued(),
        3,
        "the batch must stop at the failure, leaving the retained output queued"
    );
    h.close();
}

/// A generated reply ends the batch, because native work is then outstanding and
/// `poll_inflight_operations` has to see it before more bytes are fed.
#[test]
fn a_generated_reply_stops_the_output_batch() {
    let h = Harness::standard();
    // The second chunk makes the engine produce a reply; chunks after it must
    // not be fed in the same run.
    assert_eq!(h.owner.stage_output(b"ab"), OutputAcceptance::Accepted);
    assert_eq!(h.owner.stage_output(b"?"), OutputAcceptance::Accepted);
    assert_eq!(h.owner.stage_output(b"cd"), OutputAcceptance::Accepted);

    h.step();
    assert_eq!(
        h.owner.status().processed.offset,
        3,
        "the batch must stop at the chunk that generated a reply"
    );
    assert!(h.owner.workspace.lock().unwrap().reply.is_some());
    assert_eq!(h.owner.queue.queued(), 1);

    h.pump();
    assert_eq!(h.owner.status().processed.offset, 5);
    h.close();
}
