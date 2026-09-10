//! Real pinned native terminal rendering, mode and palette contract evidence.
//! Checkpoint, restore and history evidence lives in `terminal_state_contract.rs`.
#![cfg(feature = "ghostty")]
#[path = "fixtures/terminal_support.rs"]
mod support;
use pty_runtime_application::terminal::ITerminalFactory;
use pty_runtime_domain::terminal::*;
use pty_runtime_infrastructure::terminal::GhosttyTerminalFactory;
use support::config;

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
        t.resize(
            TerminalSize::new(40, 10).unwrap(),
            ControlGeneration::from_raw(2)
        ),
        Err(TerminalError::StaleControl)
    );
    assert_eq!(t.view().unwrap().size, config().size);
    t.resize(
        TerminalSize::new(40, 10).unwrap(),
        ControlGeneration::from_raw(1),
    )
    .unwrap();
    assert_eq!(t.view().unwrap().size, TerminalSize::new(40, 10).unwrap());
    assert_eq!(
        t.resize(config().size, ControlGeneration::from_raw(1)),
        Err(TerminalError::StaleControl)
    );
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
fn palette_and_default_color_overrides_are_domain_values() {
    let mut t = GhosttyTerminalFactory.create(config()).unwrap();
    t.feed(b"\x1b]4;1;rgb:12/34/56\x07\x1b]10;rgb:ab/cd/ef\x07\x1b[31mR")
        .unwrap();
    let view = t.view().unwrap();
    assert_eq!(view.cells[0].style.foreground, TerminalColor::Palette(1));
    assert_eq!(view.palette.indexed[1], [0x12, 0x34, 0x56]);
    assert_eq!(view.palette.foreground, Some([0xab, 0xcd, 0xef]));
}
