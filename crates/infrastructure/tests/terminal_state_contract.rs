//! Real pinned native terminal checkpoint, restore and history contract evidence.
//! Rendering, mode and palette evidence lives in `terminal_contract.rs`.
#![cfg(feature = "ghostty")]
#[path = "fixtures/terminal_support.rs"]
mod support;
use pty_runtime_application::terminal::ITerminalFactory;
use pty_runtime_domain::terminal::*;
use pty_runtime_infrastructure::terminal::GhosttyTerminalFactory;
use support::{canonical, complete, config, descriptor, same_semantics};

#[test]
fn checkpoints_preserve_parser_continuation_and_uninterrupted_state() {
    let cases: &[(&[u8], &[u8])] = &[
        (b"", b"next\r\n"),
        (b"\xe2\x82", b"\xac currency\r\n"),
        (b"\x1b[38;2;80;", b"100;200mstyled\x1b[0m\r\n"),
        (b"\x1b]2;partial title", b" finished\x1b\\visible\r\n"),
        (b"\x1bP1;2qpayload", b" more\x1b\\visible\r\n"),
        (
            b"\x1b[?1049h\x1b[?1000h\x1b[31",
            b"malternate\x1b[?1049lprimary\r\n",
        ),
    ];
    for (prefix, suffix) in cases {
        let mut reference = GhosttyTerminalFactory.create(config()).unwrap();
        for i in 0..500 {
            reference
                .feed(format!("history line {i:04}: preserved contents\r\n").as_bytes())
                .unwrap();
        }
        reference.feed(prefix).unwrap();
        let checkpoint = reference
            .checkpoint(descriptor(1000, ControlGeneration::from_raw(0)))
            .unwrap();
        let original = checkpoint.bytes.clone();
        let mut restored = GhosttyTerminalFactory
            .restore(checkpoint, config())
            .unwrap();
        assert_eq!(restored.restoration_progress(), RestorationProgress::Usable);
        assert_eq!(restored.view().unwrap(), reference.view().unwrap());
        assert!(matches!(
            restored.checkpoint(descriptor(1000, ControlGeneration::from_raw(0))),
            Err(TerminalError::HistoryIncomplete)
        ));
        complete(restored.as_mut());
        assert!(
            restored
                .checkpoint(descriptor(1000, ControlGeneration::from_raw(0)))
                .unwrap()
                .bytes
                == original,
            "immediate binary roundtrip differs"
        );
        for _ in 0..3 {
            let checkpoint = restored
                .checkpoint(descriptor(1000, ControlGeneration::from_raw(0)))
                .unwrap();
            restored = GhosttyTerminalFactory
                .restore(checkpoint, config())
                .unwrap();
            complete(restored.as_mut());
        }
        reference.feed(suffix).unwrap();
        restored.feed(suffix).unwrap();
        reference
            .resize(
                TerminalSize::new(100, 30).unwrap(),
                ControlGeneration::from_raw(1),
            )
            .unwrap();
        restored
            .resize(
                TerminalSize::new(100, 30).unwrap(),
                ControlGeneration::from_raw(1),
            )
            .unwrap();
        assert_eq!(reference.view().unwrap(), restored.view().unwrap());
        assert_eq!(
            reference.feed(b"\x1b[6n").unwrap(),
            restored.feed(b"\x1b[6n").unwrap()
        );
        same_semantics(
            reference.as_mut(),
            restored.as_mut(),
            2000,
            ControlGeneration::from_raw(1),
        );
    }
}

#[test]
fn ready_accepts_live_output_while_preserving_one_hundred_thousand_history_lines() {
    let mut c = config();
    c.history_bytes = 128 * 1024 * 1024;
    c.native_bytes = 256 * 1024 * 1024;
    let mut reference = GhosttyTerminalFactory.create(c).unwrap();
    for i in 0..100_000 {
        reference
            .feed(format!("line {i} retained history\r\n").as_bytes())
            .unwrap();
    }
    reference.feed(b"\x1b[3").unwrap();
    let checkpoint = reference
        .checkpoint(descriptor(5000, ControlGeneration::from_raw(0)))
        .unwrap();
    let formatted = canonical(checkpoint.clone());
    assert_eq!(
        formatted
            .windows(b"retained history".len())
            .filter(|window| *window == b"retained history")
            .count(),
        100_000
    );
    drop(formatted);
    let mut restored = GhosttyTerminalFactory.restore(checkpoint, c).unwrap();
    assert_eq!(restored.restoration_progress(), RestorationProgress::Usable);
    assert_eq!(restored.view().unwrap(), reference.view().unwrap());
    for t in [&mut reference, &mut restored] {
        t.feed(b"1mLIVE\x1b[0m\r\n").unwrap();
    }
    assert_eq!(restored.restoration_progress(), RestorationProgress::Usable);
    let mut steps = 0;
    while !restored.restoration_progress().is_finished() {
        restored.restore_history_step().unwrap();
        steps += 1;
        for t in [&mut reference, &mut restored] {
            t.feed(b"interleaved live output\r\n").unwrap();
        }
        assert!(steps < 10_000);
    }
    assert!(steps > 1);
    assert_eq!(
        restored.restoration_progress(),
        RestorationProgress::Complete
    );
    same_semantics(
        reference.as_mut(),
        restored.as_mut(),
        6000,
        ControlGeneration::from_raw(0),
    );
    for t in [&mut reference, &mut restored] {
        t.resize(
            TerminalSize::new(100, 30).unwrap(),
            ControlGeneration::from_raw(1),
        )
        .unwrap();
    }
    assert_eq!(restored.view().unwrap(), reference.view().unwrap());
    same_semantics(
        reference.as_mut(),
        restored.as_mut(),
        6000,
        ControlGeneration::from_raw(1),
    );
}

#[test]
fn terminal_owner_can_move_threads_and_drop_with_pending_history() {
    let mut terminal = GhosttyTerminalFactory.create(config()).unwrap();
    terminal.feed(b"move across threads").unwrap();
    let terminal = std::thread::spawn(move || {
        terminal.feed(b"\r\nsecond thread").unwrap();
        terminal
    })
    .join()
    .unwrap();
    drop(terminal);
    for _ in 0..20 {
        let mut t = GhosttyTerminalFactory.create(config()).unwrap();
        for _ in 0..200 {
            t.feed(b"retained history\r\n").unwrap();
        }
        let checkpoint = t
            .checkpoint(descriptor(0, ControlGeneration::from_raw(0)))
            .unwrap();
        drop(
            GhosttyTerminalFactory
                .restore(checkpoint, config())
                .unwrap(),
        );
    }
}
#[test]
fn compression_preserves_wide_combining_text_and_full_history() {
    let mut t = GhosttyTerminalFactory.create(config()).unwrap();
    for _ in 0..1000 {
        t.feed("wide 界 and combining e\u{301}\r\n".as_bytes())
            .unwrap();
    }
    let original = canonical(
        t.checkpoint(descriptor(0, ControlGeneration::from_raw(0)))
            .unwrap(),
    );
    let mut done = false;
    for _ in 0..10_000 {
        if t.compress_history_step().unwrap() {
            done = true;
            break;
        }
    }
    assert!(done);
    assert!(
        canonical(
            t.checkpoint(descriptor(0, ControlGeneration::from_raw(0)))
                .unwrap()
        ) == original
    );
    t.feed(b"\x1b[H").unwrap();
    t.feed("界e\u{301}".as_bytes()).unwrap();
    let view = t.view().unwrap();
    assert_eq!(view.cells[0].text, "界");
    assert_eq!(view.cells[0].width, 2);
    assert_eq!(view.cells[1].width, 0);
    assert_eq!(view.cells[2].text, "e\u{301}");
}
