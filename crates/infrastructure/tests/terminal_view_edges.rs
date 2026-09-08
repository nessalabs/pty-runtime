//! Public view conversion edges identified by own-C source coverage.
#![cfg(feature = "ghostty")]
use pty_runtime_application::terminal::ITerminalFactory;
use pty_runtime_domain::terminal::{TerminalConfig, TerminalError, TerminalSize};
use pty_runtime_infrastructure::terminal::GhosttyTerminalFactory;

fn config() -> TerminalConfig {
    TerminalConfig::new(TerminalSize::new(4, 2).unwrap())
}

#[test]
fn osc_palette_overrides_and_reset_preserve_optional_default_and_cursor_colors() {
    let mut terminal = GhosttyTerminalFactory.create(config()).unwrap();
    let initial = terminal.view().unwrap().palette;
    terminal.feed(b"\x1b]10;#112233\x1b\\\x1b]11;#445566\x1b\\\x1b]12;#778899\x1b\\\x1b]4;2;#aabbcc\x1b\\").unwrap();
    let changed = terminal.view().unwrap().palette;
    assert_eq!(changed.foreground, Some([0x11, 0x22, 0x33]));
    assert_eq!(changed.background, Some([0x44, 0x55, 0x66]));
    assert_eq!(changed.cursor, Some([0x77, 0x88, 0x99]));
    assert_eq!(changed.indexed[2], [0xaa, 0xbb, 0xcc]);
    terminal
        .feed(b"\x1b]110\x1b\\\x1b]111\x1b\\\x1b]112\x1b\\\x1b]104;2\x1b\\")
        .unwrap();
    assert_eq!(terminal.view().unwrap().palette, initial);
}

#[test]
fn long_grapheme_retries_copy_with_exact_codepoints_and_respects_view_budget() {
    let grapheme = format!("a{}", "\u{0301}".repeat(64));
    let mut terminal = GhosttyTerminalFactory.create(config()).unwrap();
    terminal.feed(grapheme.as_bytes()).unwrap();
    let view = terminal.view().unwrap();
    assert_eq!(view.cells[0].text, grapheme);
    assert_eq!(view.cells[0].width, 1);
    assert_eq!(view.cursor.col, 1);
    let mut bounded = config();
    bounded.view_bytes = 128;
    let mut terminal = GhosttyTerminalFactory.create(bounded).unwrap();
    terminal.feed(grapheme.as_bytes()).unwrap();
    assert_eq!(terminal.view(), Err(TerminalError::BudgetExceeded));
    // Failed copying must leave the authoritative terminal healthy and mutable.
    terminal.feed(b"\r\x1b[2Kok").unwrap();
    assert_eq!(terminal.view().unwrap().cells[0].text, "o");
}
