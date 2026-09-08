//! Real pinned native terminal contract evidence.
#![cfg(feature = "ghostty")]
use pty_runtime_application::terminal::ITerminalFactory;
use pty_runtime_domain::{ReplayCursor, SessionLifetime, terminal::*};
use pty_runtime_infrastructure::terminal::GhosttyTerminalFactory;
fn config() -> TerminalConfig {
    TerminalConfig {
        size: TerminalSize::new(80, 24).unwrap(),
        history_bytes: 1024 * 1024,
        continuation_bytes: 64 * 1024,
        reply_bytes: 4096,
        checkpoint_bytes: 8 * 1024 * 1024,
        feed_bytes: 64 * 1024,
        native_bytes: 32 * 1024 * 1024,
        view_bytes: 1024 * 1024,
    }
}
fn descriptor() -> CheckpointDescriptor {
    CheckpointDescriptor {
        compatibility: GhosttyTerminalFactory.compatibility().into(),
        processed: ReplayCursor {
            lifetime: SessionLifetime::new(1, 1),
            offset: 0,
        },
        control_generation: 0,
    }
}
#[test]
fn rejects_invalid_dimensions_config_and_incompatible_checkpoints() {
    assert!(TerminalSize::new(0, 24).is_err());
    assert!(TerminalSize::new(80, 0).is_err());
    assert!(TerminalSize::new(u16::MAX, u16::MAX).is_err());
    let mut c = config();
    c.reply_bytes = 0;
    assert!(matches!(
        GhosttyTerminalFactory.create(c),
        Err(TerminalError::InvalidConfiguration)
    ));
    let mut t = GhosttyTerminalFactory.create(config()).unwrap();
    let mut checkpoint = t.checkpoint(descriptor()).unwrap();
    checkpoint.descriptor.compatibility = "other-engine".into();
    assert!(matches!(
        GhosttyTerminalFactory.restore(checkpoint, config()),
        Err(TerminalError::IncompatibleCheckpoint)
    ));
    let mut bad_descriptor = descriptor();
    bad_descriptor.control_generation = 1;
    assert!(matches!(
        t.checkpoint(bad_descriptor),
        Err(TerminalError::StaleControl)
    ));
}
#[test]
fn chunk_admission_does_not_mutate_and_reply_overflow_invalidates_projection() {
    let mut c = config();
    c.feed_bytes = 4;
    c.reply_bytes = 4;
    let mut t = GhosttyTerminalFactory.create(c).unwrap();
    assert_eq!(t.feed(b"12345"), Err(TerminalError::BudgetExceeded));
    assert_eq!(t.view().unwrap().cursor.col, 0);
    assert_eq!(t.feed(b"\x1b[6n"), Err(TerminalError::BudgetExceeded));
    assert_eq!(t.feed(b"X"), Err(TerminalError::EngineFailure));
    assert_eq!(t.view(), Err(TerminalError::EngineFailure));
}
#[test]
fn checkpoint_and_native_allocations_are_bounded() {
    let mut c = config();
    c.native_bytes = 1;
    assert!(GhosttyTerminalFactory.create(c).is_err());
    c = config();
    c.checkpoint_bytes = 16;
    let mut t = GhosttyTerminalFactory.create(c).unwrap();
    assert!(matches!(
        t.checkpoint(descriptor()),
        Err(TerminalError::BudgetExceeded)
    ));
    t.feed(b"still usable after bounded writer rejection")
        .unwrap();
    let mut source = GhosttyTerminalFactory.create(config()).unwrap();
    source.feed(b"native allocation restore bound").unwrap();
    let cp = source.checkpoint(descriptor()).unwrap();
    c = config();
    c.native_bytes = 1;
    assert!(matches!(
        GhosttyTerminalFactory.restore(cp, c),
        Err(TerminalError::BudgetExceeded)
    ));
}
#[test]
fn corrupted_and_truncated_snapshots_never_complete_successfully() {
    let mut t = GhosttyTerminalFactory.create(config()).unwrap();
    for _ in 0..500 {
        t.feed(b"history retained across READY\r\n").unwrap();
    }
    let original = t.checkpoint(descriptor()).unwrap();
    for change in 0..4 {
        let mut checkpoint = original.clone();
        match change {
            0 => {
                checkpoint.bytes.truncate(8);
            }
            1 => {
                checkpoint.bytes.pop();
            }
            2 => checkpoint.bytes[20] ^= 1,
            _ => checkpoint.bytes.extend_from_slice(b"trailing bytes"),
        }
        match GhosttyTerminalFactory.restore(checkpoint, config()) {
            Err(TerminalError::CorruptCheckpoint) => (),
            Ok(mut restored) => {
                let mut failed = false;
                for _ in 0..10_000 {
                    match restored.restore_history_step() {
                        Err(TerminalError::CorruptCheckpoint) => {
                            failed = true;
                            break;
                        }
                        Ok(RestorationProgress::Usable) => (),
                        other => panic!("corruption unexpectedly accepted: {other:?}"),
                    }
                }
                assert!(failed);
                assert_eq!(
                    restored.feed(b"must fail"),
                    Err(TerminalError::EngineFailure)
                );
            }
            Err(other) => panic!("unexpected category: {other:?}"),
        }
    }
}
#[test]
fn view_and_parser_continuation_have_explicit_budgets() {
    let mut c = config();
    c.view_bytes = 4;
    c.continuation_bytes = 8;
    let mut t = GhosttyTerminalFactory.create(c).unwrap();
    t.feed(b"abc").unwrap();
    assert_eq!(t.view(), Err(TerminalError::BudgetExceeded));
    t.feed(b"\x1b]2;unfinished long title").unwrap();
    assert!(t.checkpoint(descriptor()).is_err());
}
