//! Retained-history projection evidence. See ADR 0006.
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

fn descriptor(offset: u64, generation: u64) -> CheckpointDescriptor {
    CheckpointDescriptor {
        compatibility: CompatibilityId::new(GhosttyTerminalFactory.compatibility()).unwrap(),
        processed: ReplayCursor {
            lifetime: SessionLifetime::new(1, 1),
            offset,
        },
        control_generation: generation,
    }
}

fn row_text(history: &TerminalHistory, row: usize) -> String {
    let cols = usize::from(history.cols);
    history.cells[row * cols..(row + 1) * cols]
        .iter()
        .map(|cell| {
            if cell.text.is_empty() {
                " "
            } else {
                &cell.text
            }
        })
        .collect::<String>()
        .trim_end()
        .to_owned()
}

#[test]
fn history_reads_retained_rows_without_disturbing_the_terminal() {
    let mut t = GhosttyTerminalFactory.create(config()).unwrap();
    for line in 0..200 {
        t.feed(format!("line {line}\r\n").as_bytes()).unwrap();
    }

    let all = t.history(0, u16::MAX).unwrap();
    assert!(all.total > u64::from(config().size.rows()));
    assert!(
        all.scrollback > 0,
        "200 lines must have produced scrollback"
    );
    assert_eq!(all.cols, config().size.cols());

    // Oldest first, and addressing the same row twice returns the same row.
    assert_eq!(row_text(&all, 0), "line 0");
    let one = t.history(5, 1).unwrap();
    assert_eq!(one.start, 5);
    assert_eq!(row_text(&one, 0), row_text(&all, 5));

    // The read is genuinely free of side effects: the active screen, the
    // cursor, and a later checkpoint must be indistinguishable from a run that
    // never asked for history at all.
    let before = t.view().unwrap();
    let checkpoint_before = t.checkpoint(descriptor(0, 0)).unwrap().bytes.clone();
    let _ = t.history(0, 50).unwrap();
    let _ = t.history(all.total.saturating_sub(2), 10).unwrap();
    assert_eq!(t.view().unwrap(), before);
    assert_eq!(
        t.checkpoint(descriptor(0, 0)).unwrap().bytes,
        checkpoint_before
    );

    // A request past the window is clamped rather than refused, and reports
    // where it actually began.
    let past = t.history(all.total + 1000, 4).unwrap();
    assert_eq!(past.start, past.total);
    assert!(past.cells.is_empty());

    // Live output continues to work afterwards and grows the window.
    t.feed(b"after history\r\n").unwrap();
    assert!(t.history(0, 1).unwrap().total >= all.total);
}
