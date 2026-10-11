//! Public coordinator failures at protocol admission and cleanup boundaries.
use super::{
    program_status::{report, scripted, stage, status_options},
    program_status_terminal::Script,
    support::*,
};
use crate::projection::{ProjectionError, ProjectionLimits};
use pty_runtime_domain::terminal::{
    TerminalError,
    program_status::{PROGRAM_STATUS_REPLY, ProgramState, ProgramStatusStore},
};
use std::sync::Arc;

#[test]
fn snapshots_refuse_both_closing_and_closed_sessions() {
    let h = Harness::new(status_options(), ProjectionLimits::default());
    report(&h, "state=done:msg=b2s=");
    h.pump();
    let mut closing = h.owner.close().unwrap();
    assert!(matches!(
        h.owner.program_status(),
        Err(ProjectionError::Closed)
    ));
    h.pump();
    result(&mut closing).unwrap();
    assert!(matches!(
        h.owner.program_status(),
        Err(ProjectionError::Closed)
    ));
    drop(closing);
    assert_budgets_released(&h);
}

#[test]
fn a_rejected_snapshot_releases_its_admission_and_does_not_change_the_record() {
    let h = Harness::new(status_options(), ProjectionLimits::default());
    report(&h, "state=done:msg=b2s=");
    h.pump();
    let usage = h.budgets.resources().views;
    let occupied = usage.limit - usage.used;
    assert!(h.budgets.views.acquire(occupied));
    assert!(matches!(
        h.owner.program_status(),
        Err(ProjectionError::Capacity)
    ));
    assert_eq!(h.budgets.resources().views.used, usage.limit);
    h.budgets.views.release(occupied);
    let snapshot = h.owner.program_status().unwrap();
    assert_eq!(snapshot.records()[0].msg.as_deref(), Some("ok"));
    assert_eq!(snapshot.revision(), 1);
    drop(snapshot);
    assert_eq!(h.budgets.resources().views.used, usage.used);
    h.close();
    assert_budgets_released(&h);
}

#[test]
fn a_scan_that_fits_but_cannot_retain_its_record_fails_before_native_feed() {
    let options = status_options();
    let bytes = b"\x1b]7501;state=done\x07";
    let domain = ProgramStatusStore::default();
    let limit = domain.scan_reservation_for(bytes).unwrap()
        + domain.record_reservation()
        + options.terminal.reply_bytes
        - 1;
    let script = Arc::new(Script::default());
    let h = scripted(
        options,
        ProjectionLimits {
            view_bytes: limit,
            ..ProjectionLimits::default()
        },
        script.clone(),
    );
    stage(&h, bytes);
    h.pump();
    assert_eq!(h.owner.status().failure, Some(ProjectionError::Capacity));
    assert_eq!(h.owner.status().processed.offset, 0);
    assert!(script.feeds.lock().unwrap().is_empty());
    assert_eq!(
        h.budgets.resources().views.used,
        0,
        "temporary scan admission must be released"
    );
    h.close();
    assert_budgets_released(&h);
}

#[test]
fn record_or_reply_admission_failure_rolls_back_the_already_admitted_scan() {
    let bytes = b"\x1b]7501;state=done\x07\x1b]7501;?\x07";
    let domain = ProgramStatusStore::default();
    let scratch = domain.scan_reservation_for(bytes).unwrap();
    for reach_reply_admission in [false, true] {
        let script = Arc::new(Script::default());
        let h = scripted(
            status_options(),
            ProjectionLimits::default(),
            script.clone(),
        );
        let available = scratch
            + if reach_reply_admission {
                domain.record_reservation()
            } else {
                0
            };
        let occupied = h.budgets.resources().views.limit - available;
        assert!(h.budgets.views.acquire(occupied));
        stage(&h, bytes);
        h.step();
        assert_eq!(h.owner.status().failure, None);
        assert_eq!(h.owner.status().processed.offset, 0);
        assert!(script.feeds.lock().unwrap().is_empty());
        assert_eq!(
            h.budgets.resources().views.used,
            occupied,
            "all provisional reservations must roll back"
        );
        let unchanged = h.owner.program_status().unwrap();
        assert!(unchanged.records().is_empty());
        assert_eq!(unchanged.revision(), 0);
        drop(unchanged);
        h.budgets.views.release(occupied);
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
}

#[test]
fn a_mutating_query_ending_slice_failure_never_attempts_later_slices_or_replies() {
    let script = Arc::new(Script {
        fail_at: Some(1),
        ..Script::default()
    });
    let h = scripted(
        status_options(),
        ProjectionLimits::default(),
        script.clone(),
    );
    let first = b"\x1b]7501;state=done\x07\x1b]7501;?\x07";
    stage(&h, &[first.as_slice(), b"\x1b[c"].concat());
    h.pump();
    assert_eq!(
        h.owner.status().failure,
        Some(ProjectionError::Terminal(TerminalError::HistoryIncomplete))
    );
    assert_eq!(h.owner.status().processed.offset, 0);
    for _ in 0..3 {
        h.step();
    }
    assert_eq!(*script.feeds.lock().unwrap(), [first.to_vec()]);
    assert!(h.process.writes.lock().unwrap().is_empty());
    h.close();
    assert_budgets_released(&h);
}
