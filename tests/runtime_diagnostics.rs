//! Internal boundaries are measured independently from child echo round trips.
mod support;
use pty_runtime::*;
#[cfg(feature = "ghostty")]
use std::time::{Duration, Instant};

#[test]
fn raw_measurements_count_admission_actual_io_and_rejected_input_separately() {
    let diagnostics = RuntimeDiagnostics::new();
    let owner = support::runtime(RuntimeOptions::default()).with_diagnostics(diagnostics.clone());
    let session = owner
        .spawn(
            support::id("timed-raw"),
            &support::command("echo", &[]),
            support::options(),
        )
        .unwrap();
    let mut observer = session.attach(AttachPosition::Oldest).unwrap();
    let _ = support::block_on(observer.read_next()).unwrap();
    let outcome = support::block_on(session.write(b"measured\n").unwrap());
    assert_eq!(outcome.written, 9);
    assert_eq!(outcome.error, None);
    let _ = support::block_on(observer.read_next()).unwrap();
    support::block_on(session.resize(TerminalSize::new(81, 25).unwrap()).unwrap()).unwrap();
    assert!(session.write(&vec![0; 1024 * 1024]).is_err());
    session.cancel().unwrap();
    support::block_on(session.wait().unwrap()).unwrap();
    owner.shutdown();
    assert_eq!(
        diagnostics.snapshot(LatencyKind::InputAdmission).samples(),
        1
    );
    assert_eq!(
        diagnostics.snapshot(LatencyKind::InputAdmission).failures,
        1
    );
    assert_eq!(
        diagnostics.snapshot(LatencyKind::InputDispatch).samples(),
        1
    );
    assert_eq!(
        diagnostics.snapshot(LatencyKind::ResizeDispatch).samples(),
        1
    );
    assert_eq!(
        diagnostics.snapshot(LatencyKind::ResizeAdmission).samples(),
        1
    );
    assert_eq!(
        diagnostics.snapshot(LatencyKind::CancelAdmission).samples(),
        1
    );
    assert!(diagnostics.snapshot(LatencyKind::RawOutput).samples() >= 2);
    assert_eq!(
        diagnostics.snapshot(LatencyKind::ProjectedOutput).samples(),
        0
    );
}

#[cfg(feature = "ghostty")]
#[test]
fn projected_samples_finish_after_native_feed_and_ordered_resize() {
    let diagnostics = RuntimeDiagnostics::new();
    let owner = support::runtime(RuntimeOptions::default()).with_diagnostics(diagnostics.clone());
    let size = TerminalSize::new(80, 24).unwrap();
    let session = owner
        .spawn(
            support::id("timed-model"),
            &support::command("echo", &[]),
            SessionOptions::projected(terminal::TerminalConfig::new(size)),
        )
        .unwrap();
    let mut observer = session.attach(AttachPosition::Oldest).unwrap();
    let _ = support::block_on(observer.read_next()).unwrap();
    support::block_on(session.write(b"\x1b[31mmeasured\x1b[0m\n").unwrap());
    let _ = support::block_on(observer.read_next()).unwrap();
    let resize = support::block_on(
        session
            .resize_projected(TerminalSize::new(81, 25).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert!(resize.os.is_ok());
    assert!(resize.model.is_ok());
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let status = session.projection_status().unwrap().unwrap();
        if status.processed == status.published {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    session.cancel().unwrap();
    support::block_on(session.wait().unwrap()).unwrap();
    owner.shutdown();
    assert!(diagnostics.snapshot(LatencyKind::ProjectedOutput).samples() >= 2);
    assert_eq!(
        diagnostics.snapshot(LatencyKind::ProjectedOutput).failures,
        0
    );
    assert_eq!(
        diagnostics.snapshot(LatencyKind::ResizeDispatch).samples(),
        1
    );
    assert_eq!(
        diagnostics.snapshot(LatencyKind::ResizeDispatch).failures,
        0
    );
}

#[test]
fn cancellation_dispatch_requires_acknowledged_workload_and_verified_groups() {
    let diagnostics = RuntimeDiagnostics::new();
    let owner = support::runtime(RuntimeOptions::default()).with_diagnostics(diagnostics.clone());
    let cwd = std::env::current_dir().unwrap();
    let command = CommandSpec::new(
        "/bin/sh".into(),
        cwd,
        vec!["-c".into(), "trap '' TERM; printf ready; sleep 30".into()],
    )
    .unwrap();
    let session = owner
        .spawn(support::id("timed-cancel"), &command, support::options())
        .unwrap();
    let mut observer = session.attach(AttachPosition::Oldest).unwrap();
    let _ = support::block_on(observer.read_next()).unwrap();
    session.cancel().unwrap();
    let completion = support::block_on(session.wait().unwrap()).unwrap();
    assert_eq!(completion.status.exit, Some(ExitStatus::Signal(9)));
    assert!(completion.status.supervision_error.is_none());
    owner.shutdown();
    let dispatch = diagnostics.snapshot(LatencyKind::CancelDispatch);
    assert_eq!(dispatch.samples(), 1);
    assert_eq!(dispatch.failures, 0);
    assert_eq!(dispatch.unavailable, 0);
    assert_eq!(
        diagnostics
            .aggregate()
            .count(CounterKind::AcknowledgedWorkloadEscalations),
        1
    );
}

#[test]
fn aggregate_gauges_follow_completed_handles_and_exact_replay_gaps() {
    let diagnostics = RuntimeDiagnostics::new();
    let owner = support::runtime(RuntimeOptions::default()).with_diagnostics(diagnostics.clone());
    let mut options = support::options();
    options.replay_bytes = 8;
    let first_id = support::id("retention-first");
    let second_id = support::id("retention-second");
    let first = owner
        .spawn(
            first_id.clone(),
            &support::command("bytes", &["512"]),
            options.clone(),
        )
        .unwrap();
    let second = owner
        .spawn(
            second_id.clone(),
            &support::command("bytes", &["512"]),
            options,
        )
        .unwrap();
    support::block_on(first.wait().unwrap()).unwrap();
    support::block_on(second.wait().unwrap()).unwrap();
    owner.shutdown();
    let aggregate = diagnostics.aggregate();
    assert_eq!(aggregate.active_sessions, 0);
    assert_eq!(aggregate.retained_replay_bytes, 16);
    assert_eq!(aggregate.count(CounterKind::BytesRead), 1024);
    assert_eq!(aggregate.count(CounterKind::PublishedBytes), 1024);
    assert_eq!(aggregate.count(CounterKind::CleanupCompleted), 2);
    assert_eq!(aggregate.count(CounterKind::CleanupFailed), 0);
    diagnostics.reset_quiescent();
    assert_eq!(diagnostics.aggregate().retained_replay_bytes, 16);
    assert_eq!(diagnostics.aggregate().count(CounterKind::BytesRead), 0);
    let mut observers = [first.clone(), second.clone()].map(|session| {
        session
            .attach(AttachPosition::Cursor(ReplayCursor {
                lifetime: session.lifetime(),
                offset: 0,
            }))
            .unwrap()
    });
    for observer in &mut observers {
        assert!(matches!(
            observer.try_next().unwrap(),
            Some(OutputEvent::Replay(ReplayPage::Gap { .. }))
        ));
    }
    assert_eq!(owner.resources().observers.used, 2);
    assert_eq!(owner.resources().replay_capacity.used, 16);
    assert_eq!(diagnostics.aggregate().count(CounterKind::ObserverGaps), 2);
    assert_eq!(
        diagnostics.aggregate().count(CounterKind::ObserverGapBytes),
        1008
    );
    owner.forget(&first_id).unwrap();
    owner.forget(&second_id).unwrap();
    drop(first);
    drop(second);
    assert_eq!(diagnostics.aggregate().retained_replay_bytes, 16);
    drop(observers);
    assert_eq!(diagnostics.aggregate().retained_replay_bytes, 0);
    assert_eq!(owner.resources().observers.used, 0);
    assert_eq!(owner.resources().replay_capacity.used, 0);
    assert_eq!(owner.resources().input_bytes.used, 0);
    assert_eq!(owner.resources().input_slots.used, 0);
}
