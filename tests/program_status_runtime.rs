//! Real PTY bytes, Ghostty reply ordering, and session-owned status lifecycle.
#![cfg(feature = "ghostty")]
#[allow(dead_code)]
mod support;
use pty_runtime::ports::ITerminalFactory;
use pty_runtime::{
    terminal::{TerminalConfig, program_status::ProgramState},
    *,
};
use pty_runtime_infrastructure::terminal::GhosttyTerminalFactory;
use std::time::{Duration, Instant};
use support::*;

const REPORTS: &[u8] = b"\x1b]7501;state=idle:app=fixture\x07\x1b]7501;state=working:id=job:progress=25\x1b\\\x1b]7501;state=blocked:id=permission:kind=permission\x07\x1b]7501;state=done:id=finished\x07\x1b]7501;state=error:id=failed\x07";
const QUERY: &[u8] = b"\x1b]7501;?\x1b\\";
const TAIL: &[u8] = b"\x1b]7501;state=working:id=tail\x07STATUS_EXIT";

fn read_ready(reader: &mut Attachment) -> Vec<u8> {
    let mut output = Vec::new();
    while !output.ends_with(b"STATUS_READY") {
        match block_on(reader.read_next()).unwrap() {
            OutputEvent::Replay(ReplayPage::Bytes { bytes, .. }) => output.extend(bytes),
            event => panic!("child failed before handshake: {event:?}"),
        }
    }
    output
}
fn finish(session: &Session, reader: &mut Attachment, output: &mut Vec<u8>) {
    assert_eq!(block_on(session.write(b"done").unwrap()).written, 4);
    loop {
        match block_on(reader.read_next()).unwrap() {
            OutputEvent::Replay(ReplayPage::Bytes { bytes, .. }) => output.extend(bytes),
            OutputEvent::Complete(done) => {
                assert_eq!(done.status.exit, Some(ExitStatus::Code(0)));
                assert_eq!(done.status.drain, Some(DrainOutcome::Eof));
                break;
            }
            event => panic!("unexpected output: {event:?}"),
        }
    }
}
#[test]
fn projected_status_reports_and_query_replies_preserve_original_bytes_and_exit_cleanup() {
    let owner = runtime(RuntimeOptions::default());
    let session = owner
        .spawn(
            id("status-projected"),
            &command("program-status", &[]),
            SessionOptions::projected(TerminalConfig::new(TerminalSize::new(80, 24).unwrap())),
        )
        .unwrap();
    let mut first = session.attach(AttachPosition::Oldest).unwrap();
    let mut second = session.attach(AttachPosition::Oldest).unwrap();
    let mut output = read_ready(&mut first);
    assert_eq!(
        output,
        read_ready(&mut second),
        "observers cannot generate replies"
    );
    assert_eq!(output, [REPORTS, QUERY, b"\x1b[cSTATUS_READY"].concat());
    let snapshot = session.program_status().unwrap().unwrap();
    assert_eq!(snapshot.records().len(), 5);
    let job = snapshot.records().iter().find(|r| r.id == "job").unwrap();
    assert_eq!(job.state, ProgramState::Working);
    assert_eq!(job.progress, Some(25));
    assert_eq!(job.resolved_app.as_deref(), Some("fixture"));
    let permission = snapshot
        .records()
        .iter()
        .find(|r| r.id == "permission")
        .unwrap();
    assert_eq!(permission.state, ProgramState::Blocked);
    let mut reference = GhosttyTerminalFactory
        .create(TerminalConfig::new(TerminalSize::new(80, 24).unwrap()))
        .unwrap();
    reference.feed(b"STATUS_READY").unwrap();
    assert_eq!(
        block_on(session.projected_view().unwrap()).unwrap().view(),
        &reference.view().unwrap(),
        "status reports and detection query must not alter the terminal view"
    );
    let transfer = block_on(session.begin_transfer().unwrap()).unwrap();
    let (_, mut continuation) = transfer.into_parts();
    let mut cursor = continuation.boundary().cursor;
    let old_revision = snapshot.revision();
    finish(&session, &mut first, &mut output);
    assert_eq!(
        output,
        [REPORTS, QUERY, b"\x1b[cSTATUS_READY", TAIL].concat()
    );
    let mut transferred = Vec::new();
    loop {
        match block_on(continuation.wait(cursor)).unwrap() {
            TransferRead::Event(event) => {
                match event.kind() {
                    TransferEventKind::Output(bytes) => transferred.extend_from_slice(bytes),
                    TransferEventKind::Resize { .. } => panic!("no resize was requested"),
                }
                cursor = event.after().cursor;
            }
            TransferRead::End(end) => {
                assert_eq!(end.projection_failure, None);
                break;
            }
            TransferRead::Pending => panic!("wait must suspend while pending"),
        }
    }
    assert_eq!(
        transferred, TAIL,
        "transfer preserves the original status sequence"
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let status = session.program_status().unwrap().unwrap();
        let projection = session.projection_status().unwrap().unwrap();
        assert_eq!(projection.failure, None);
        if projection.processed.offset as usize == output.len() && status.records().len() == 3 {
            assert_eq!(
                status.records().iter().map(|r| r.state).collect::<Vec<_>>(),
                [ProgramState::Idle, ProgramState::Done, ProgramState::Error]
            );
            assert!(status.revision() > old_revision);
            break;
        }
        assert!(
            Instant::now() < deadline,
            "ordered exit/final drain did not settle"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    owner.shutdown();
    assert_eq!(
        snapshot.records().len(),
        5,
        "consumer snapshot survives cleanup"
    );
}
#[test]
fn raw_status_bytes_are_passive_and_never_get_automatic_replies() {
    let owner = runtime(RuntimeOptions::default());
    let session = owner
        .spawn(
            id("status-raw"),
            &command("program-status-raw", &[]),
            options(),
        )
        .unwrap();
    let mut reader = session.attach(AttachPosition::Oldest).unwrap();
    let mut output = read_ready(&mut reader);
    assert_eq!(
        session.program_status().unwrap().map(|s| s.revision()),
        None
    );
    assert_eq!(output, [REPORTS, QUERY, b"STATUS_READY"].concat());
    finish(&session, &mut reader, &mut output);
    assert_eq!(output, [REPORTS, QUERY, b"STATUS_READY", TAIL].concat());
    owner.shutdown();
}

fn wait_projection(session: &Session, condition: impl Fn(ProjectionStatus) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let status = session.projection_status().unwrap().unwrap();
        assert_eq!(status.failure, None);
        if condition(status) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "projection did not reach expected state"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn real_parking_retains_status_and_status_reads_do_not_restore_native_model() {
    let owner = runtime(RuntimeOptions::default());
    let mut settings =
        SessionOptions::projected(TerminalConfig::new(TerminalSize::new(80, 24).unwrap()));
    settings.projection.as_mut().unwrap().park_after = Duration::from_millis(50);
    let session = owner
        .spawn(id("status-parking"), &command("echo", &[]), settings)
        .unwrap();
    let mut reader = session.attach(AttachPosition::Oldest).unwrap();
    let mut output = Vec::new();
    while !output.ends_with(b"ready") {
        match block_on(reader.read_next()).unwrap() {
            OutputEvent::Replay(ReplayPage::Bytes { bytes, .. }) => output.extend(bytes),
            event => panic!("echo fixture did not start: {event:?}"),
        }
    }
    let report = b"\x1b]7501;state=done:id=retained:app=fixture\x07";
    assert_eq!(
        block_on(session.write(report).unwrap()).written,
        report.len()
    );
    while output.len() < 5 + report.len() {
        match block_on(reader.read_next()).unwrap() {
            OutputEvent::Replay(ReplayPage::Bytes { bytes, .. }) => output.extend(bytes),
            event => panic!("echo fixture did not report: {event:?}"),
        }
    }
    assert_eq!(output, [b"ready".as_slice(), report].concat());
    drop(reader);
    wait_projection(&session, |s| s.residency == Residency::Parked);
    let before = session.program_status().unwrap().unwrap();
    assert_eq!(before.records().len(), 1);
    assert_eq!(before.records()[0].state, ProgramState::Done);
    assert_eq!(
        session.projection_status().unwrap().unwrap().residency,
        Residency::Parked
    );
    // A real copied view restores the engine; the protocol remains independently owned.
    block_on(session.projected_view().unwrap()).unwrap();
    let after = session.program_status().unwrap().unwrap();
    assert_eq!(before.revision(), after.revision());
    assert_eq!(before.records(), after.records());
    wait_projection(&session, |s| s.residency == Residency::Parked);
    session.cancel().unwrap();
    block_on(session.wait().unwrap()).unwrap();
    assert_eq!(
        session.program_status().unwrap().unwrap().records(),
        before.records()
    );
    assert_eq!(
        session.projection_status().unwrap().unwrap().residency,
        Residency::Parked
    );
    owner.shutdown();
}
