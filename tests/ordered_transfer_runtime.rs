//! Real Unix PTY + Ghostty checkpoint/ordered continuation consumer equality.
#![cfg(feature = "ghostty")]
#[allow(dead_code)]
mod support;
use pty_runtime::{
    ports::{ITerminal, ITerminalFactory},
    terminal::{RestorationProgress, TerminalCheckpoint, TerminalConfig},
    *,
};
use pty_runtime_infrastructure::terminal::GhosttyTerminalFactory;
use std::time::{Duration, Instant};
use support::*;
fn output_through(attachment: &mut Attachment, marker: &[u8]) {
    let mut bytes = Vec::new();
    loop {
        match block_on(attachment.read_next()).unwrap() {
            OutputEvent::Replay(ReplayPage::Bytes { bytes: chunk, .. }) => {
                bytes.extend(chunk);
                if bytes.windows(marker.len()).any(|part| part == marker) {
                    return;
                }
            }
            other => panic!("unexpected raw outcome {other:?}"),
        }
    }
}
fn restore(
    transfer: StateTransfer,
    config: TerminalConfig,
) -> (Box<dyn ITerminal>, TransferObserver) {
    let (pin, observer) = transfer.into_parts();
    let state = pin.checkpoint();
    assert_eq!(state.descriptor.processed, observer.boundary().processed);
    assert_eq!(
        state.descriptor.control_generation,
        observer.boundary().control_generation
    );
    let mut terminal = GhosttyTerminalFactory
        .restore(
            TerminalCheckpoint {
                descriptor: state.descriptor.clone(),
                bytes: state.bytes.clone(),
            },
            config,
        )
        .unwrap();
    while terminal.restoration_progress() != RestorationProgress::Complete {
        terminal.restore_history_step().unwrap();
    }
    (terminal, observer)
}
#[test]
fn two_parked_snapshot_consumers_replay_original_bytes_and_resizes_to_identical_final_models() {
    let owner = runtime(RuntimeOptions::default());
    let config = TerminalConfig::new(TerminalSize::new(80, 24).unwrap());
    let mut options = SessionOptions::projected(config);
    options.projection.as_mut().unwrap().park_after = Duration::from_millis(30);
    let session = owner
        .spawn(id("ordered-transfer"), &command("echo", &[]), options)
        .unwrap();
    let mut raw = session.attach(AttachPosition::Oldest).unwrap();
    output_through(&mut raw, b"ready");
    // Park a model with unfinished UTF-8 and substantial prior history.
    let mut before = b"\x1b[2J\x1b[H".to_vec();
    for _ in 0..48 {
        before.extend_from_slice(b"history row\r\n");
    }
    before.extend_from_slice(b"\xf0\x9f");
    block_on(session.write(&before).unwrap());
    output_through(&mut raw, b"\xf0\x9f");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if session.projection_status().unwrap().unwrap().residency == Residency::Parked {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    let first = block_on(session.begin_transfer().unwrap()).unwrap();
    let second = block_on(session.begin_transfer().unwrap()).unwrap();
    assert_eq!(
        session.projection_status().unwrap().unwrap().residency,
        Residency::Parked
    );
    assert_eq!(first.boundary(), second.boundary());
    let (mut one, mut observer_one) = restore(first, config);
    let (mut two, mut observer_two) = restore(second, config);
    let suffix = b"\x98\x80 first\x1b[";
    block_on(session.write(suffix).unwrap());
    output_through(&mut raw, b"\x1b[");
    let first_resize = block_on(
        session
            .resize_projected(TerminalSize::new(90, 28).unwrap())
            .unwrap(),
    )
    .unwrap();
    let second_resize = block_on(
        session
            .resize_projected(TerminalSize::new(70, 20).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(first_resize.model, Ok(()));
    assert_eq!(second_resize.model, Ok(()));
    let suffix = b"31mred\x1b[0m\x1b[?1049halt\x1b[?1049l\x1b[6n";
    block_on(session.write(suffix).unwrap());
    // The sole authoritative reply is echoed by the real child. Replicas discard
    // their own generated replies while applying this same query below.
    output_through(&mut raw, b"R");
    session.cancel().unwrap();
    let process = block_on(session.wait().unwrap()).unwrap();
    assert!(process.status.drain.is_some());
    let mut cursors = [
        observer_one.boundary().cursor,
        observer_two.boundary().cursor,
    ];
    let mut controls = Vec::new();
    for (index, (model, observer)) in [(&mut one, &mut observer_one), (&mut two, &mut observer_two)]
        .into_iter()
        .enumerate()
    {
        let mut generated_replies = 0;
        loop {
            match block_on(observer.wait(cursors[index])).unwrap() {
                TransferRead::Event(event) => {
                    match event.kind() {
                        TransferEventKind::Output(bytes) => {
                            let effects = model.feed(bytes).unwrap();
                            if !effects.0.is_empty() {
                                generated_replies += 1;
                            }
                            // Deliberately do not send effects to the PTY.
                        }
                        TransferEventKind::Resize { size, generation } => {
                            model.resize(size, generation).unwrap();
                            if index == 0 {
                                controls.push(event.after());
                            }
                        }
                    }
                    cursors[index] = event.after().cursor;
                }
                TransferRead::End(end) => {
                    assert_eq!(end.drain, process.status.drain);
                    assert_eq!(end.projection_failure, None);
                    assert_eq!(end.boundary.cursor, cursors[index]);
                    break;
                }
                TransferRead::Pending => panic!("wait returned pending"),
            }
        }
        assert_eq!(generated_replies, 1);
    }
    assert_eq!(controls.len(), 2);
    assert_eq!(controls[0].processed, controls[1].processed);
    assert_eq!(controls[0].cursor.sequence + 1, controls[1].cursor.sequence);
    let authoritative = block_on(session.projected_view().unwrap()).unwrap();
    assert_eq!(&one.view().unwrap(), authoritative.view());
    assert_eq!(&two.view().unwrap(), authoritative.view());
    owner.shutdown();
    assert!(matches!(
        observer_one.read(cursors[0]),
        Err(TransferError::Closed)
    ));
}
