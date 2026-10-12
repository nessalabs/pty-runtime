//! Independent acceptance of session-owned protocol state and native feed transactions.
use super::{
    program_status_terminal::{Factory, Script},
    support::*,
    terminal::Trace,
};
use crate::{
    process::OutputAcceptance,
    projection::{ProjectionError, ProjectionLimits, ProjectionOptions, Residency},
};
use pty_runtime_domain::{
    process::DrainOutcome,
    terminal::{
        TerminalError,
        program_status::{PROGRAM_STATUS_REPLY, ProgramState},
    },
};
use std::sync::{Arc, atomic::Ordering};

pub(super) fn status_options() -> ProjectionOptions {
    let mut options = options();
    options.terminal.feed_bytes = 4096;
    options
}
pub(super) fn scripted(
    options: ProjectionOptions,
    limits: ProjectionLimits,
    script: Arc<Script>,
) -> Harness {
    Harness::with_services(options, limits, move |services| {
        services.terminal = Arc::new(Factory {
            inner: services.terminal.clone(),
            script,
        });
    })
}
pub(super) fn stage(h: &Harness, bytes: &[u8]) {
    assert_eq!(h.owner.stage_output(bytes), OutputAcceptance::Accepted);
}
pub(super) fn report(h: &Harness, body: &str) {
    stage(h, format!("\x1b]7501;{body}\x07").as_bytes());
}

#[test]
fn queries_split_native_feeds_and_preserve_reply_order_with_following_da1() {
    for terminator in ["\x07", "\x1b\\"] {
        let script = Arc::new(Script {
            replies: vec![b"native-before".to_vec(), b"DA1".to_vec()],
            ..Script::default()
        });
        let h = scripted(
            status_options(),
            ProjectionLimits::default(),
            script.clone(),
        );
        let query = format!("\x1b]7501;?{terminator}");
        let chunk = format!("{query}\x1b[c");
        stage(&h, chunk.as_bytes());
        h.pump();
        let mut expected = b"native-before".to_vec();
        expected.extend_from_slice(PROGRAM_STATUS_REPLY);
        expected.extend_from_slice(b"DA1");
        assert_eq!(*h.process.writes.lock().unwrap(), [expected]);
        assert_eq!(
            *script.feeds.lock().unwrap(),
            [query.into_bytes(), b"\x1b[c".to_vec()]
        );
        assert_eq!(h.owner.status().processed.offset, chunk.len() as u64);
        assert!(h.owner.program_status().unwrap().records().is_empty());
        h.close();
        assert_budgets_released(&h);
    }
}

#[test]
fn query_split_across_output_chunks_replies_once_and_reports_generate_none() {
    let script = Arc::new(Script::default());
    let h = scripted(status_options(), ProjectionLimits::default(), script);
    stage(&h, b"\x1b]7501;?\x1b");
    h.pump();
    assert!(h.process.writes.lock().unwrap().is_empty());
    stage(&h, b"\\");
    report(&h, "state=done:msg=b2s=");
    h.pump();
    assert_eq!(
        *h.process.writes.lock().unwrap(),
        [PROGRAM_STATUS_REPLY.to_vec()]
    );
    assert_eq!(
        h.owner.program_status().unwrap().records()[0]
            .msg
            .as_deref(),
        Some("ok")
    );
    h.close();
}

#[test]
fn a_mutating_second_slice_failure_is_terminal_and_never_replays_or_publishes_chunk() {
    let script = Arc::new(Script {
        fail_at: Some(2),
        ..Script::default()
    });
    let h = scripted(
        status_options(),
        ProjectionLimits::default(),
        script.clone(),
    );
    let prefix = b"\x1b]7501;state=working\x07\x1b]7501;?\x07";
    let suffix = b"\x1b]7501;state=done\x07";
    let bytes = [prefix.as_slice(), suffix].concat();
    stage(&h, &bytes);
    h.pump();
    assert_eq!(
        h.owner.status().failure,
        Some(ProjectionError::Terminal(TerminalError::HistoryIncomplete))
    );
    assert_eq!(h.owner.status().processed.offset, 0);
    assert_eq!(h.owner.status().published.offset, bytes.len() as u64);
    assert!(h.owner.program_status().is_err());
    for _ in 0..4 {
        h.step();
    }
    assert_eq!(
        *script.feeds.lock().unwrap(),
        [prefix.to_vec(), suffix.to_vec()]
    );
    assert!(h.process.writes.lock().unwrap().is_empty());
    h.close();
    assert_budgets_released(&h);
}

#[test]
fn native_and_protocol_replies_share_one_aggregate_limit_without_partial_delivery() {
    for native_reply in [Vec::new(), vec![b'R']] {
        let fits = native_reply.is_empty();
        let mut options = status_options();
        options.terminal.reply_bytes = 2 * PROGRAM_STATUS_REPLY.len();
        let script = Arc::new(Script {
            replies: vec![native_reply.clone(), native_reply],
            ..Script::default()
        });
        let h = scripted(options, ProjectionLimits::default(), script);
        let bytes = b"\x1b]7501;?\x07\x1b]7501;?\x07";
        stage(&h, bytes);
        h.pump();
        if fits {
            assert_eq!(h.owner.status().failure, None);
            assert_eq!(h.owner.status().processed.offset, bytes.len() as u64);
            assert_eq!(
                *h.process.writes.lock().unwrap(),
                [PROGRAM_STATUS_REPLY.repeat(2)]
            );
        } else {
            assert_eq!(h.owner.status().failure, Some(ProjectionError::Capacity));
            assert_eq!(h.owner.status().processed.offset, 0);
            assert!(h.process.writes.lock().unwrap().is_empty());
        }
        h.close();
        assert_budgets_released(&h);
    }
}

#[test]
fn exit_marker_survives_full_parser_and_control_admission_and_is_ordered() {
    for shared in [false, true] {
        let mut options = status_options();
        options.staging_slots = if shared { 32 } else { 2 };
        options.request_slots = if shared { 32 } else { 1 };
        let h = Harness::new(
            options,
            ProjectionLimits {
                staging_slots: if shared { 2 } else { 32 },
                request_slots: if shared { 1 } else { 32 },
                ..ProjectionLimits::default()
            },
        );
        report(&h, "state=done:id=kept");
        h.pump();
        report(&h, "state=working:id=work");
        report(&h, "state=blocked:id=blocked");
        let held = h.owner.view().unwrap();
        assert!(matches!(h.owner.view(), Err(ProjectionError::Capacity)));
        assert_eq!(
            h.owner.stage_output(b"full"),
            OutputAcceptance::Backpressure
        );
        h.owner.notify_process_exited();
        h.owner.notify_process_exited();
        h.pump();
        let snapshot = h.owner.program_status().unwrap();
        assert_eq!(snapshot.records().len(), 1);
        assert_eq!(snapshot.records()[0].id, "kept");
        drop((snapshot, held));
        h.close();
        assert_budgets_released(&h);
    }
}

#[test]
fn final_drain_clears_reports_buffered_after_exit_and_exit_after_drain_also_cleans() {
    for drain_first in [false, true] {
        let h = Harness::new(status_options(), ProjectionLimits::default());
        report(&h, "state=working:id=early");
        h.pump();
        if !drain_first {
            h.owner.notify_process_exited();
            h.pump();
        }
        report(&h, "state=working:id=late");
        report(&h, "state=blocked:id=blocked");
        report(&h, "state=done:id=done");
        report(&h, "state=error:id=error");
        h.owner.notify_output_drained(DrainOutcome::Eof);
        if drain_first {
            h.pump();
            h.owner.notify_process_exited();
        }
        h.pump();
        let snapshot = h.owner.program_status().unwrap();
        assert_eq!(
            snapshot
                .records()
                .iter()
                .map(|record| record.state)
                .collect::<Vec<_>>(),
            [ProgramState::Done, ProgramState::Error]
        );
        let revision = snapshot.revision();
        drop(snapshot);
        h.owner.notify_process_exited();
        h.owner.notify_output_drained(DrainOutcome::Eof);
        h.pump();
        assert_eq!(h.owner.program_status().unwrap().revision(), revision);
        h.close();
    }
}

#[test]
fn parked_reads_and_exit_cleanup_do_not_restore_the_native_model() {
    let h = Harness::new(status_options(), ProjectionLimits::default());
    report(&h, "state=working:id=work");
    report(&h, "state=done:id=done");
    h.pump();
    let before = h.owner.program_status().unwrap();
    h.park();
    let resources = h.budgets.resources().native_reservations;
    let occupied = resources.limit - resources.used;
    assert!(h.budgets.resident.acquire(occupied));
    let parked = h.owner.program_status().unwrap();
    assert_eq!(parked.records(), before.records());
    assert_eq!(parked.revision(), before.revision());
    h.owner.notify_process_exited();
    h.pump();
    assert_eq!(h.owner.status().residency, Residency::Parked);
    assert_eq!(h.probe.alive.load(Ordering::Acquire), 0);
    assert!(
        !h.probe
            .trace
            .lock()
            .unwrap()
            .iter()
            .any(|trace| matches!(trace, Trace::Restore(..)))
    );
    let after = h.owner.program_status().unwrap();
    assert_eq!(after.records().len(), 1);
    assert_eq!(after.records()[0].state, ProgramState::Done);
    drop((before, parked, after));
    h.budgets.resident.release(occupied);
    h.close();
    assert_budgets_released(&h);
}

#[test]
fn snapshot_quota_remains_charged_after_session_close_until_consumer_drops_it() {
    let h = Harness::new(status_options(), ProjectionLimits::default());
    report(&h, "state=done:msg=c2Vuc2l0aXZl");
    h.pump();
    let before = h.budgets.resources().views.used;
    let snapshot = h.owner.program_status().unwrap();
    let snapshot_charge = h.budgets.resources().views.used - before;
    assert!(snapshot_charge > 0);
    assert!(!format!("{snapshot:?}").contains("sensitive"));
    h.close();
    assert_eq!(h.budgets.resources().views.used, snapshot_charge);
    assert_eq!(snapshot.records()[0].msg.as_deref(), Some("sensitive"));
    drop(snapshot);
    assert_budgets_released(&h);
}

#[test]
fn protocol_admission_pressure_leaves_native_and_scan_state_untouched_until_retry() {
    let script = Arc::new(Script::default());
    let h = scripted(
        status_options(),
        ProjectionLimits::default(),
        script.clone(),
    );
    let limit = h.budgets.resources().views.limit;
    assert!(h.budgets.views.acquire(limit));
    stage(&h, b"\x1b]7501;state=done\x07\x1b]7501;?\x07");
    h.step();
    assert_eq!(h.owner.status().processed.offset, 0);
    assert_eq!(h.owner.status().failure, None);
    assert!(script.feeds.lock().unwrap().is_empty());
    h.budgets.views.release(limit);
    h.pump();
    assert_eq!(
        h.owner.program_status().unwrap().records()[0].state,
        ProgramState::Done
    );
    assert_eq!(
        *h.process.writes.lock().unwrap(),
        [PROGRAM_STATUS_REPLY.to_vec()]
    );
    h.close();
    assert_budgets_released(&h);
}

#[test]
fn impossible_protocol_memory_budget_fails_without_feeding_or_retrying_forever() {
    let script = Arc::new(Script::default());
    let h = scripted(
        status_options(),
        ProjectionLimits {
            view_bytes: 4096,
            ..ProjectionLimits::default()
        },
        script.clone(),
    );
    report(&h, "state=done");
    h.pump();
    assert_eq!(h.owner.status().failure, Some(ProjectionError::Capacity));
    assert!(script.feeds.lock().unwrap().is_empty());
    h.close();
    assert_budgets_released(&h);
}

#[test]
fn ordinary_output_keeps_program_status_storage_lazy() {
    let h = Harness::new(status_options(), ProjectionLimits::default());
    assert_eq!(h.budgets.resources().views.used, 0);
    stage(&h, b"ordinary output without escape sequences");
    h.pump();
    assert_eq!(h.budgets.resources().views.used, 0);
    h.close();
    assert_budgets_released(&h);
}

#[test]
fn record_cap_configuration_rejects_values_outside_protocol_range() {
    for cap in [0, 63, 257, usize::MAX] {
        let mut options = status_options();
        options.program_status_records = cap;
        assert_eq!(
            options.validate().unwrap_err(),
            ProjectionError::InvalidConfiguration
        );
    }
}

#[test]
fn restoring_native_checkpoint_preserves_records_and_revision() {
    let h = Harness::new(status_options(), ProjectionLimits::default());
    report(&h, "state=done:app=deploy:msg=b2s=");
    h.pump();
    let before = h.owner.program_status().unwrap();
    h.park();
    let mut view = h.owner.view().unwrap();
    h.pump();
    result(&mut view).unwrap();
    let restored = h.owner.program_status().unwrap();
    assert_eq!(restored.records(), before.records());
    assert_eq!(restored.revision(), before.revision());
    assert!(
        h.probe
            .trace
            .lock()
            .unwrap()
            .iter()
            .any(|trace| matches!(trace, Trace::Restore(..)))
    );
    drop((before, restored, view));
    h.close();
    assert_budgets_released(&h);
}

#[test]
fn status_snapshot_does_not_wait_for_a_blocked_native_checkpoint() {
    use std::{
        sync::{Barrier, mpsc},
        time::Duration,
    };
    let h = Harness::new(status_options(), ProjectionLimits::default());
    report(&h, "state=done:msg=b2s=");
    h.pump();
    let barrier = Arc::new(Barrier::new(2));
    *h.probe.encode_barrier.lock().unwrap() = Some(barrier.clone());
    h.clock.0.store(60, Ordering::Release);
    let native_owner = h.owner.clone();
    let native_worker = std::thread::spawn(move || {
        use crate::scheduling::IScheduledWork;
        native_owner.run()
    });
    barrier.wait();
    let (sender, receiver) = mpsc::channel();
    let status_owner = h.owner.clone();
    let status_worker = std::thread::spawn(move || {
        sender
            .send(
                status_owner
                    .program_status()
                    .map(|snapshot| snapshot.records()[0].state),
            )
            .unwrap();
    });
    let outcome = receiver.recv_timeout(Duration::from_secs(2));
    // Release the injected block even if the observation violated its contract.
    barrier.wait();
    native_worker.join().unwrap();
    status_worker.join().unwrap();
    assert_eq!(outcome, Ok(Ok(ProgramState::Done)));
    h.pump();
    h.close();
    assert_budgets_released(&h);
}
