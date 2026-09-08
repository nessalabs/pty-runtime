//! Payload-bearing portable values must be safe in ordinary diagnostic formatting.
use crate::{
    ReplayCursor, ReplayPage, SessionLifetime,
    checkpoint::{CheckpointKey, ProtectedCheckpoint},
    terminal::{
        CheckpointDescriptor, TerminalCell, TerminalCheckpoint, TerminalColor, TerminalStyle,
        Underline,
    },
};

const MARKER: &str = "synthetic-private-terminal-content";

fn does_not_disclose(value: &dyn std::fmt::Debug) {
    for rendered in [format!("{value:?}"), format!("{value:#?}")] {
        assert!(!rendered.contains(MARKER));
        assert!(!rendered.contains(&format!("{:?}", MARKER.as_bytes())));
    }
}

#[test]
fn opaque_checkpoints_hide_payload_and_adapter_metadata_in_debug() {
    let lifetime = SessionLifetime::new(8, 3);
    let descriptor = CheckpointDescriptor {
        compatibility: MARKER.into(),
        processed: ReplayCursor {
            lifetime,
            offset: 42,
        },
        control_generation: 7,
    };
    let checkpoint = TerminalCheckpoint {
        descriptor: descriptor.clone(),
        bytes: MARKER.as_bytes().to_vec(),
    };
    let protected = ProtectedCheckpoint::new(
        CheckpointKey {
            lifetime,
            generation: 9,
        },
        descriptor,
        MARKER.as_bytes().to_vec(),
    );
    does_not_disclose(&checkpoint);
    does_not_disclose(&protected);
    assert!(format!("{checkpoint:?}").contains(&MARKER.len().to_string()));
    assert!(format!("{protected:?}").contains(&MARKER.len().to_string()));
}

#[test]
fn replay_and_cell_debug_redact_content_while_retaining_safe_position_and_style() {
    let lifetime = SessionLifetime::new(8, 3);
    let page = ReplayPage::Bytes {
        from: ReplayCursor {
            lifetime,
            offset: 0,
        },
        next: ReplayCursor {
            lifetime,
            offset: MARKER.len() as u64,
        },
        bytes: MARKER.as_bytes().to_vec(),
    };
    let cell = TerminalCell {
        text: MARKER.into(),
        width: 1,
        style: TerminalStyle {
            foreground: TerminalColor::Rgb(1, 2, 3),
            background: TerminalColor::Default,
            underline_color: TerminalColor::Palette(4),
            underline: Underline::Curly,
            bold: true,
            italic: false,
            faint: false,
            blink: false,
            inverse: false,
            invisible: false,
            strikethrough: false,
            overline: false,
        },
    };
    does_not_disclose(&page);
    does_not_disclose(&cell);
    let page_debug = format!("{page:?}");
    assert!(page_debug.contains("from"));
    assert!(page_debug.contains("next"));
    let cell_debug = format!("{cell:?}");
    assert!(cell_debug.contains("width: 1"));
    assert!(cell_debug.contains("Curly"));
    assert!(cell_debug.contains("Rgb(1, 2, 3)"));
}
