//! Snapshot parity for pending wrap, custom margins and saved cursor state.
#![cfg(feature = "ghostty")]
use pty_runtime_application::terminal::{ITerminal, ITerminalFactory};
use pty_runtime_domain::{ReplayCursor, SessionLifetime, terminal::*};
use pty_runtime_infrastructure::terminal::GhosttyTerminalFactory;

fn config(cols: u16) -> TerminalConfig {
    TerminalConfig {
        size: TerminalSize::new(cols, 12).unwrap(),
        history_bytes: 16 * 1024 * 1024,
        continuation_bytes: 64 * 1024,
        reply_bytes: 4096,
        checkpoint_bytes: 8 * 1024 * 1024,
        feed_bytes: 4096,
        native_bytes: 32 * 1024 * 1024,
        view_bytes: 1024 * 1024,
    }
}
fn restore(t: &mut dyn ITerminal, c: TerminalConfig, generation: u64) -> Box<dyn ITerminal> {
    let descriptor = CheckpointDescriptor {
        compatibility: CompatibilityId::new(GhosttyTerminalFactory.compatibility()).unwrap(),
        processed: ReplayCursor {
            lifetime: SessionLifetime::new(1, 1),
            offset: 0,
        },
        control_generation: generation,
    };
    let cp = t.checkpoint(descriptor).unwrap();
    let mut restored = GhosttyTerminalFactory.restore(cp, c).unwrap();
    for _ in 0..4096 {
        let progress = restored.restore_history_step().unwrap();
        if progress.is_finished() {
            assert_eq!(progress, RestorationProgress::Complete);
            return restored;
        }
    }
    panic!("bounded fixture history did not finish");
}
fn same_after(a: &mut dyn ITerminal, b: &mut dyn ITerminal, bytes: &[u8]) {
    assert_eq!(a.feed(bytes).unwrap(), b.feed(bytes).unwrap());
    let left = a.view().unwrap();
    let right = b.view().unwrap();
    assert!(
        left == right,
        "continuation changed cursor {:?} versus {:?}",
        left.cursor,
        right.cursor
    );
}

#[test]
fn alternate_pending_wrap_survives_snapshot_after_widening() {
    let mut c = config(53);
    let mut t = GhosttyTerminalFactory.create(c).unwrap();
    t.feed(b"\x1b[?1049h").unwrap();
    t.feed(&[b'x'; 53]).unwrap();
    c.size = TerminalSize::new(100, 15).unwrap();
    t.resize(c.size, 1).unwrap();
    let mut restored = restore(t.as_mut(), c, 1);
    same_after(t.as_mut(), restored.as_mut(), b"Z");
}

#[test]
fn custom_right_margin_pending_wrap_survives_snapshot() {
    let c = config(10);
    let mut t = GhosttyTerminalFactory.create(c).unwrap();
    t.feed(b"\x1b[?69h\x1b[1;5sabcde").unwrap();
    assert_eq!(t.view().unwrap().cursor.col, 4);
    assert!(t.view().unwrap().cursor.pending_wrap);
    let mut restored = restore(t.as_mut(), c, 0);
    same_after(t.as_mut(), restored.as_mut(), b"Z\x1b[6n");
}

#[test]
fn saved_cursor_pending_wrap_at_custom_margin_survives_snapshot() {
    let c = config(10);
    let mut t = GhosttyTerminalFactory.create(c).unwrap();
    t.feed(b"\x1b[?69h\x1b[1;5sabcde\x1b7\x1b[H").unwrap();
    assert!(!t.view().unwrap().cursor.pending_wrap);
    let mut restored = restore(t.as_mut(), c, 0);
    same_after(t.as_mut(), restored.as_mut(), b"\x1b8Z\x1b[6n");
}

#[test]
fn full_width_pending_wrap_retains_existing_behavior() {
    let c = config(10);
    let mut t = GhosttyTerminalFactory.create(c).unwrap();
    t.feed(b"0123456789").unwrap();
    assert!(t.view().unwrap().cursor.pending_wrap);
    let mut restored = restore(t.as_mut(), c, 0);
    same_after(t.as_mut(), restored.as_mut(), b"Z\x1b[6n");
}

fn nonreflow_pair(alternate: bool) -> (Box<dyn ITerminal>, Box<dyn ITerminal>) {
    let mut c = config(100);
    let mut original = GhosttyTerminalFactory.create(c).unwrap();
    original
        .feed(if alternate {
            b"\x1b[?1049h"
        } else {
            b"\x1b[?7l"
        })
        .unwrap();
    c.size = TerminalSize::new(59, 12).unwrap();
    original.resize(c.size, 1).unwrap();
    original.feed(b"\x1b[?7h\x1b[1;59HALT").unwrap();
    let restored = restore(original.as_mut(), c, 1);
    (original, restored)
}

#[test]
fn primary_wrapped_rows_survive_restore_before_nonreflow_growth_and_later_reflow() {
    let (mut original, mut restored) = nonreflow_pair(false);
    for terminal in [&mut original, &mut restored] {
        terminal.feed(b"\x1b[?7l").unwrap();
        terminal
            .resize(TerminalSize::new(88, 12).unwrap(), 2)
            .unwrap();
        terminal.feed(b"\x1b[?7h").unwrap();
        terminal
            .resize(TerminalSize::new(40, 12).unwrap(), 3)
            .unwrap();
    }
    same_after(original.as_mut(), restored.as_mut(), b"Z\x1b[6n");
    let view = original.view().unwrap();
    assert_eq!((view.cursor.col, view.cursor.row), (3, 2));
    assert_eq!(view.cells[80].text, "L");
    assert_eq!(view.cells[81].text, "T");
    assert_eq!(view.cells[82].text, "Z");
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
fn full_state(t: &mut dyn ITerminal, generation: u64) -> Vec<u8> {
    let cp = t
        .checkpoint(CheckpointDescriptor {
            compatibility: CompatibilityId::new(GhosttyTerminalFactory.compatibility()).unwrap(),
            processed: ReplayCursor {
                lifetime: SessionLifetime::new(1, 1),
                offset: 0,
            },
            control_generation: generation,
        })
        .unwrap();
    let mut out = vec![0; 4 * 1024 * 1024];
    let mut len = 0;
    // SAFETY: The synchronous oracle borrows the checkpoint and initialized
    // output buffer only for this call, respects capacity, and retains no pointers.
    assert_eq!(
        unsafe {
            rt_verify_format(
                cp.bytes.as_ptr(),
                cp.bytes.len(),
                out.as_mut_ptr(),
                out.len(),
                &mut len,
            )
        },
        0
    );
    assert!(len <= out.len());
    out.truncate(len);
    out
}

#[test]
fn alternate_wrapped_rows_survive_restore_before_nonreflow_growth() {
    let (mut original, mut restored) = nonreflow_pair(true);
    for terminal in [&mut original, &mut restored] {
        terminal
            .resize(TerminalSize::new(88, 12).unwrap(), 2)
            .unwrap();
    }
    same_after(original.as_mut(), restored.as_mut(), b"Z\x1b[6n");
    let left = full_state(original.as_mut(), 2);
    let right = full_state(restored.as_mut(), 2);
    assert!(
        left == right,
        "alternate wrapped-row state changed after restore and resize"
    );
}

fn wide_cutoff_roundtrip(alternate: bool, glyph: &str) {
    let mut c = config(54);
    let mut original = GhosttyTerminalFactory.create(c).unwrap();
    original
        .feed(if alternate {
            b"\x1b[?1049h"
        } else {
            b"\x1b[?7l"
        })
        .unwrap();
    original.feed(b"\x1b[1;27H").unwrap();
    original.feed(glyph.as_bytes()).unwrap();
    assert_eq!(original.view().unwrap().cells[26].width, 2);
    c.size = TerminalSize::new(27, 12).unwrap();
    original.resize(c.size, 1).unwrap();
    // Checkpoint must be possible immediately after clipping, before input
    // could overwrite and hide an orphaned wide base at the new last column.
    let mut restored = restore(original.as_mut(), c, 1);
    for terminal in [&mut original, &mut restored] {
        let view = terminal.view().unwrap();
        assert_eq!(view.cells[26].text, "");
        assert_eq!(view.cells[26].width, 1);
    }
    same_after(original.as_mut(), restored.as_mut(), b"\x1b[?7hZQ\x1b[6n");
    let view = original.view().unwrap();
    assert_eq!((view.cursor.col, view.cursor.row), (1, 1));
    assert_eq!(view.cells[26].text, "Z");
    assert_eq!(view.cells[27].text, "Q");
}

#[test]
fn nonreflow_shrink_clips_wide_glyph_before_checkpoint_and_later_input() {
    for alternate in [false, true] {
        wide_cutoff_roundtrip(alternate, "界");
    }
}

#[test]
fn nonreflow_shrink_clips_styled_hyperlinked_wide_grapheme() {
    for alternate in [false, true] {
        wide_cutoff_roundtrip(
            alternate,
            "\x1b[31m\x1b]8;;https://example.test\x07界\u{301}\x1b]8;;\x07\x1b[0m",
        );
    }
}

#[test]
fn restored_inactive_viewport_pin_does_not_push_live_text_into_history_on_shrink() {
    let mut c = config(10);
    c.size = TerminalSize::new(10, 3).unwrap();
    let mut original = GhosttyTerminalFactory.create(c).unwrap();
    for _ in 0..20 {
        original.feed(b"history\r\n").unwrap();
    }
    let mut restored = restore(original.as_mut(), c, 0);
    for terminal in [&mut original, &mut restored] {
        terminal
            .resize(TerminalSize::new(10, 6).unwrap(), 1)
            .unwrap();
        terminal.feed(b"\x1b[H\x1b[2Jhello").unwrap();
        terminal
            .resize(TerminalSize::new(10, 2).unwrap(), 2)
            .unwrap();
    }
    same_after(original.as_mut(), restored.as_mut(), b"Z\x1b[6n");
    let view = original.view().unwrap();
    assert_eq!((view.cursor.col, view.cursor.row), (6, 0));
    for (cell, text) in view.cells.iter().zip(["h", "e", "l", "l", "o", "Z"]) {
        assert_eq!(cell.text, text);
    }
    assert!(full_state(original.as_mut(), 2) == full_state(restored.as_mut(), 2));
}
