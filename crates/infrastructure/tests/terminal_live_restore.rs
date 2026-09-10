//! READY is usable immediately; skipped source history must remain explicit.
#![cfg(feature = "ghostty")]
use pty_runtime_application::terminal::{ITerminal, ITerminalFactory};
use pty_runtime_domain::{ReplayCursor, SessionLifetime, terminal::*};
use pty_runtime_infrastructure::terminal::GhosttyTerminalFactory;

fn fixture() -> (TerminalConfig, TerminalCheckpoint) {
    let mut config = TerminalConfig::new(TerminalSize::new(80, 24).unwrap());
    config.history_bytes = 32 * 1024 * 1024;
    config.native_bytes = 64 * 1024 * 1024;
    let mut terminal = GhosttyTerminalFactory.create(config).unwrap();
    for _ in 0..5000 {
        terminal.feed(b"original retained history\r\n").unwrap();
    }
    terminal.feed(b"\x1b[3").unwrap();
    let checkpoint = terminal
        .checkpoint(CheckpointDescriptor {
            compatibility: CompatibilityId::new(GhosttyTerminalFactory.compatibility()).unwrap(),
            processed: ReplayCursor {
                lifetime: SessionLifetime::new(8, 1),
                offset: 8192,
            },
            control_generation: 0,
        })
        .unwrap();
    (config, checkpoint)
}
fn finish(terminal: &mut dyn ITerminal) -> RestorationProgress {
    for _ in 0..10_000 {
        let progress = terminal.restore_history_step().unwrap();
        if progress.is_finished() {
            return progress;
        }
    }
    panic!("source did not finish within its finite page bound")
}
#[test]
fn live_resize_and_quota_pressure_report_validated_but_inapplicable_history() {
    for resize in [true, false] {
        let (mut config, checkpoint) = fixture();
        if !resize {
            config.history_bytes = 0;
        }
        let mut terminal = GhosttyTerminalFactory.restore(checkpoint, config).unwrap();
        assert_eq!(terminal.restoration_progress(), RestorationProgress::Usable);
        terminal.feed(b"1mLIVE\x1b[0m").unwrap();
        if resize {
            terminal
                .resize(TerminalSize::new(91, 27).unwrap(), 1)
                .unwrap();
        }
        let first = terminal.restore_history_step().unwrap();
        if resize {
            assert!(matches!(
                first,
                RestorationProgress::UsableWithSkippedHistory { skipped_pages: 1 }
            ));
        }
        let progress = finish(terminal.as_mut());
        assert!(
            matches!(progress, RestorationProgress::FinishedWithSkippedHistory { skipped_pages } if skipped_pages > 0)
        );
        assert_ne!(progress, RestorationProgress::Complete);
        assert_eq!(terminal.restoration_progress(), progress);
        let view = terminal.view().unwrap();
        assert!(view.cells.iter().any(|cell| cell.text == "L"));
        terminal.feed(b"\x1b[6n").unwrap();
        assert_eq!(terminal.restoration_progress(), progress);
    }
}
#[test]
fn restored_live_feed_keeps_continuation_callbacks_and_reply_bounds() {
    let (mut config, checkpoint) = fixture();
    config.reply_bytes = 4;
    let mut terminal = GhosttyTerminalFactory.restore(checkpoint, config).unwrap();
    terminal.feed(b"1mLIVE\x1b[0m").unwrap();
    assert_eq!(terminal.restoration_progress(), RestorationProgress::Usable);
    assert_eq!(
        terminal.feed(b"\x1b[6n"),
        Err(TerminalError::BudgetExceeded)
    );
    assert_eq!(terminal.view(), Err(TerminalError::EngineFailure));
}

#[test]
fn skipped_history_still_requires_valid_finish_and_rejects_future_mutation() {
    let (config, mut checkpoint) = fixture();
    checkpoint.bytes.pop(); // READY and history survive; FINISH is incomplete.
    let mut terminal = GhosttyTerminalFactory.restore(checkpoint, config).unwrap();
    terminal
        .resize(TerminalSize::new(91, 27).unwrap(), 1)
        .unwrap();
    assert_eq!(
        terminal.restore_history_step().unwrap(),
        RestorationProgress::UsableWithSkippedHistory { skipped_pages: 1 }
    );
    let mut failed = false;
    for _ in 0..10_000 {
        match terminal.restore_history_step() {
            Ok(progress) => assert!(!progress.is_finished()),
            Err(error) => {
                assert_eq!(error, TerminalError::CorruptCheckpoint);
                failed = true;
                break;
            }
        }
    }
    assert!(
        failed,
        "truncated FINISH must fail after consuming skipped pages"
    );
    assert!(!terminal.restoration_progress().is_finished());
    assert_eq!(terminal.feed(b"later"), Err(TerminalError::EngineFailure));
    assert_eq!(terminal.view(), Err(TerminalError::EngineFailure));
}
