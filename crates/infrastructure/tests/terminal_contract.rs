//! Real pinned native terminal contract evidence.
#![cfg(feature = "ghostty")]
use pty_runtime_application::terminal::{ITerminal, ITerminalFactory};
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
fn complete(terminal: &mut dyn ITerminal) {
    for _ in 0..10_000 {
        if terminal.restore_history_step().unwrap() == RestorationProgress::Complete {
            return;
        }
    }
    panic!("history did not complete within bounded corpus page count");
}

#[test]
fn incremental_text_styles_modes_cursor_and_ordered_resize() {
    let mut t = GhosttyTerminalFactory.create(config()).unwrap();
    for part in [
        b"\x1b[1;3;38;2;10;".as_slice(),
        b"20;30mA\xe2",
        b"\x82",
        b"\xac\x1b[0m",
        b"\x1b[?2004h\x1b[?1h\x1b[?1000h",
    ] {
        t.feed(part).unwrap();
    }
    let v = t.view().unwrap();
    assert_eq!(v.cells[0].text, "A");
    assert_eq!(v.cells[1].text, "€");
    assert!(v.cells[0].style.bold && v.cells[0].style.italic);
    assert_eq!(v.cells[0].style.foreground, TerminalColor::Rgb(10, 20, 30));
    assert_eq!(v.cursor.col, 2);
    assert!(v.modes.bracketed_paste && v.modes.application_cursor);
    // Mode 1000 asks for presses and releases in the original encoding. Both
    // halves are reported, not merely that something is on.
    assert!(v.modes.mouse_reporting());
    assert_eq!(v.modes.mouse, MouseTracking::PressRelease);
    assert_eq!(v.modes.mouse_encoding, MouseEncoding::Legacy);
    assert_eq!(t.feed(b"\x1b[6n").unwrap().0, b"\x1b[1;3R");
    assert_eq!(
        t.resize(TerminalSize::new(40, 10).unwrap(), 2),
        Err(TerminalError::StaleControl)
    );
    assert_eq!(t.view().unwrap().size, config().size);
    t.resize(TerminalSize::new(40, 10).unwrap(), 1).unwrap();
    assert_eq!(t.view().unwrap().size, TerminalSize::new(40, 10).unwrap());
    assert_eq!(t.resize(config().size, 1), Err(TerminalError::StaleControl));
    t.feed(b"\x1b[?1049hALT").unwrap();
    assert!(t.view().unwrap().modes.alternate_screen);
    t.feed(b"\x1b[?1049l").unwrap();
    assert!(!t.view().unwrap().modes.alternate_screen);
}

#[test]
fn mouse_tracking_and_encoding_are_reported_separately() {
    let mut t = GhosttyTerminalFactory.create(config()).unwrap();
    assert_eq!(t.view().unwrap().modes.mouse, MouseTracking::None);

    // Button-event tracking with the SGR encoding: the pair a modern program
    // asks for, and the pair a consumer cannot infer from one another.
    t.feed(b"\x1b[?1002h\x1b[?1006h").unwrap();
    let v = t.view().unwrap();
    assert_eq!(v.modes.mouse, MouseTracking::ButtonMotion);
    assert_eq!(v.modes.mouse_encoding, MouseEncoding::Sgr);

    // Any-event tracking supersedes it while the encoding is unaffected.
    t.feed(b"\x1b[?1003h").unwrap();
    let v = t.view().unwrap();
    assert_eq!(v.modes.mouse, MouseTracking::AnyMotion);
    assert_eq!(v.modes.mouse_encoding, MouseEncoding::Sgr);

    // Turning tracking off leaves nothing to report, whatever the encoding.
    t.feed(b"\x1b[?1003l\x1b[?1002l").unwrap();
    let v = t.view().unwrap();
    assert_eq!(v.modes.mouse, MouseTracking::None);
    assert!(!v.modes.mouse_reporting());
}

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
        let checkpoint = reference.checkpoint(descriptor(1000, 0)).unwrap();
        let original = checkpoint.bytes.clone();
        let mut restored = GhosttyTerminalFactory
            .restore(checkpoint, config())
            .unwrap();
        assert_eq!(restored.restoration_progress(), RestorationProgress::Usable);
        assert_eq!(restored.view().unwrap(), reference.view().unwrap());
        assert!(matches!(
            restored.checkpoint(descriptor(1000, 0)),
            Err(TerminalError::HistoryIncomplete)
        ));
        complete(restored.as_mut());
        assert!(
            restored.checkpoint(descriptor(1000, 0)).unwrap().bytes == original,
            "immediate binary roundtrip differs"
        );
        for _ in 0..3 {
            let checkpoint = restored.checkpoint(descriptor(1000, 0)).unwrap();
            restored = GhosttyTerminalFactory
                .restore(checkpoint, config())
                .unwrap();
            complete(restored.as_mut());
        }
        reference.feed(suffix).unwrap();
        restored.feed(suffix).unwrap();
        reference
            .resize(TerminalSize::new(100, 30).unwrap(), 1)
            .unwrap();
        restored
            .resize(TerminalSize::new(100, 30).unwrap(), 1)
            .unwrap();
        assert_eq!(reference.view().unwrap(), restored.view().unwrap());
        assert_eq!(
            reference.feed(b"\x1b[6n").unwrap(),
            restored.feed(b"\x1b[6n").unwrap()
        );
        same_semantics(reference.as_mut(), restored.as_mut(), 2000, 1);
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
    let checkpoint = reference.checkpoint(descriptor(5000, 0)).unwrap();
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
    same_semantics(reference.as_mut(), restored.as_mut(), 6000, 0);
    for t in [&mut reference, &mut restored] {
        t.resize(TerminalSize::new(100, 30).unwrap(), 1).unwrap();
    }
    assert_eq!(restored.view().unwrap(), reference.view().unwrap());
    same_semantics(reference.as_mut(), restored.as_mut(), 6000, 1);
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
        let checkpoint = t.checkpoint(descriptor(0, 0)).unwrap();
        drop(
            GhosttyTerminalFactory
                .restore(checkpoint, config())
                .unwrap(),
        );
    }
}

unsafe extern "C" {
    fn rt_verify_format(
        bytes: *const u8,
        len: usize,
        out: *mut u8,
        cap: usize,
        written: *mut usize,
    ) -> i32;
}
fn canonical(checkpoint: TerminalCheckpoint) -> Vec<u8> {
    let mut bytes = vec![0u8; 4 * 1024 * 1024];
    let mut len = 0;
    // SAFETY: The independent C oracle borrows complete immutable checkpoint bytes
    // and writes only within this initialized buffer during the synchronous call.
    let result = unsafe {
        rt_verify_format(
            checkpoint.bytes.as_ptr(),
            checkpoint.bytes.len(),
            bytes.as_mut_ptr(),
            bytes.len(),
            &mut len,
        )
    };
    assert_eq!(result, 0);
    bytes.truncate(len);
    bytes
}
fn same_semantics(a: &mut dyn ITerminal, b: &mut dyn ITerminal, offset: u64, generation: u64) {
    let a = canonical(a.checkpoint(descriptor(offset, generation)).unwrap());
    let b = canonical(b.checkpoint(descriptor(offset, generation)).unwrap());
    assert!(
        a == b,
        "full formatted terminal/history state differs ({} vs {} bytes)",
        a.len(),
        b.len()
    );
}

#[test]
fn compression_preserves_wide_combining_text_and_full_history() {
    let mut t = GhosttyTerminalFactory.create(config()).unwrap();
    for _ in 0..1000 {
        t.feed("wide 界 and combining e\u{301}\r\n".as_bytes())
            .unwrap();
    }
    let original = canonical(t.checkpoint(descriptor(0, 0)).unwrap());
    let mut done = false;
    for _ in 0..10_000 {
        if t.compress_history_step().unwrap() {
            done = true;
            break;
        }
    }
    assert!(done);
    assert!(canonical(t.checkpoint(descriptor(0, 0)).unwrap()) == original);
    t.feed(b"\x1b[H").unwrap();
    t.feed("界e\u{301}".as_bytes()).unwrap();
    let view = t.view().unwrap();
    assert_eq!(view.cells[0].text, "界");
    assert_eq!(view.cells[0].width, 2);
    assert_eq!(view.cells[1].width, 0);
    assert_eq!(view.cells[2].text, "e\u{301}");
}

#[test]
fn palette_and_default_color_overrides_are_domain_values() {
    let mut t = GhosttyTerminalFactory.create(config()).unwrap();
    t.feed(b"\x1b]4;1;rgb:12/34/56\x07\x1b]10;rgb:ab/cd/ef\x07\x1b[31mR")
        .unwrap();
    let view = t.view().unwrap();
    assert_eq!(view.cells[0].style.foreground, TerminalColor::Palette(1));
    assert_eq!(view.palette.indexed[1], [0x12, 0x34, 0x56]);
    assert_eq!(view.palette.foreground, Some([0xab, 0xcd, 0xef]));
}
