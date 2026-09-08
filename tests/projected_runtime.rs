//! Real PTY/Ghostty projection, encrypted parking and authoritative reply integration.
#![cfg(feature = "ghostty")]
#[allow(dead_code)]
mod support;
use pty_runtime::{
    ports::ITerminalFactory,
    terminal::{RestorationProgress, TerminalCheckpoint, TerminalConfig},
    *,
};
use pty_runtime_infrastructure::terminal::GhosttyTerminalFactory;
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use support::*;

fn bytes_until(attachment: &mut Attachment, marker: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    loop {
        match block_on(attachment.read_next()).unwrap() {
            OutputEvent::Replay(ReplayPage::Bytes { bytes: chunk, .. }) => {
                bytes.extend(chunk);
                if bytes.windows(marker.len()).any(|w| w == marker) {
                    return bytes;
                }
            }
            event => panic!("unexpected output event {event:?}"),
        }
    }
}
fn wait_residency(session: &Session, desired: Residency) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let status = session.projection_status().unwrap().unwrap();
        assert_eq!(status.failure, None);
        assert_eq!(status.parking_failure, None);
        if status.residency == desired {
            return;
        }
        assert!(Instant::now() < deadline, "projection state: {status:?}");
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn detached_real_model_parks_transfers_restores_and_resizes_without_losing_bytes() {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let parent = std::env::temp_dir().join(format!(
        "pty-projected-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&parent).unwrap();
    let owner = Runtime::with_storage(
        vec![std::env::current_dir().unwrap()],
        RuntimeOptions::default(),
        StorageOptions {
            parent: Some(parent.clone()),
            ..StorageOptions::default()
        },
    )
    .unwrap();
    let config = TerminalConfig::new(TerminalSize::new(80, 24).unwrap());
    let mut options = SessionOptions::projected(config);
    options.projection.as_mut().unwrap().park_after = Duration::from_millis(50);
    let session = owner
        .spawn(id("projected"), &command("echo", &[]), options)
        .unwrap();
    let mut attachment = session.attach(AttachPosition::Oldest).unwrap();
    let initial = bytes_until(&mut attachment, b"ready");
    let cursor = attachment.cursor();
    drop(attachment);
    let payload = "\u{1b}[2J\u{1b}[H\u{1b}[31mhello 界 e\u{301}\u{1b}[0m!".as_bytes();
    assert_eq!(
        block_on(session.write(payload).unwrap()).written,
        payload.len()
    );
    let mut attachment = owner
        .lookup(&id("projected"))
        .unwrap()
        .attach(AttachPosition::Cursor(cursor))
        .unwrap();
    let output = bytes_until(&mut attachment, b"!");
    assert_eq!(output, payload);
    let mut reference = GhosttyTerminalFactory.create(config).unwrap();
    reference.feed(&initial).unwrap();
    reference.feed(payload).unwrap();
    assert_eq!(
        block_on(session.projected_view().unwrap()).unwrap().view(),
        &reference.view().unwrap()
    );
    drop(attachment);
    wait_residency(&session, Residency::Parked);
    let transfer = block_on(session.terminal_checkpoint().unwrap()).unwrap();
    assert_eq!(
        session.projection_status().unwrap().unwrap().residency,
        Residency::Parked
    );
    assert_eq!(
        transfer.checkpoint().descriptor.processed.offset as usize,
        initial.len() + output.len()
    );
    let checkpoint = TerminalCheckpoint {
        descriptor: transfer.checkpoint().descriptor.clone(),
        bytes: transfer.checkpoint().bytes.clone(),
    };
    let mut consumer = GhosttyTerminalFactory.restore(checkpoint, config).unwrap();
    while consumer.restoration_progress() != RestorationProgress::Complete {
        consumer.restore_history_step().unwrap();
    }
    assert_eq!(consumer.view().unwrap(), reference.view().unwrap());
    drop(transfer);
    let mut attachment = session.attach(AttachPosition::Tail).unwrap();
    let suffix = b" AFTER-PARK";
    assert_eq!(
        block_on(session.write(suffix).unwrap()).written,
        suffix.len()
    );
    assert_eq!(bytes_until(&mut attachment, b"AFTER-PARK"), suffix);
    reference.feed(suffix).unwrap();
    assert_eq!(
        block_on(session.projected_view().unwrap()).unwrap().view(),
        &reference.view().unwrap()
    );
    let size = TerminalSize::new(100, 30).unwrap();
    let resized = block_on(session.resize_projected(size).unwrap()).unwrap();
    assert_eq!(resized.os, Ok(()));
    assert_eq!(resized.model, Ok(()));
    reference.resize(size, resized.generation).unwrap();
    assert_eq!(
        block_on(session.projected_view().unwrap()).unwrap().view(),
        &reference.view().unwrap()
    );
    session.cancel().unwrap();
    block_on(session.wait().unwrap()).unwrap();
    owner.shutdown();
    assert_eq!(
        session.projection_status().unwrap().unwrap().residency,
        Residency::Closed
    );
    let entries: Vec<_> = std::fs::read_dir(&parent)
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(entries.len(), 1, "only the shared private arena may remain");
    let arena = &entries[0];
    assert!(arena.file_type().unwrap().is_dir());
    assert!(
        arena
            .file_name()
            .to_str()
            .unwrap()
            .starts_with(".pty-runtime-checkpoints-v1-")
    );
    assert_eq!(
        std::fs::read_dir(arena.path()).unwrap().count(),
        0,
        "old session handles must not retain an owner namespace or ciphertext"
    );
    std::fs::remove_dir(arena.path()).unwrap();
    std::fs::remove_dir(parent).unwrap();
}
#[test]
fn real_query_reply_is_sent_once_with_multiple_observers() {
    let owner = runtime(RuntimeOptions::default());
    let config = TerminalConfig::new(TerminalSize::new(80, 24).unwrap());
    let session = owner
        .spawn(
            id("query"),
            &command("projection-query", &[]),
            SessionOptions::projected(config),
        )
        .unwrap();
    let mut first = session.attach(AttachPosition::Oldest).unwrap();
    let mut second = session.attach(AttachPosition::Oldest).unwrap();
    assert!(bytes_until(&mut first, b"REPLY_OK").ends_with(b"REPLY_OK"));
    assert!(bytes_until(&mut second, b"REPLY_OK").ends_with(b"REPLY_OK"));
    assert_eq!(block_on(session.write(b"done").unwrap()).written, 4);
    let completion = block_on(session.wait().unwrap()).unwrap();
    assert_eq!(completion.status.exit, Some(ExitStatus::Code(0)));
    assert_eq!(completion.status.drain, Some(DrainOutcome::Eof));
    owner.shutdown();
}
