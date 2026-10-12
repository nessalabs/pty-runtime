//! Public behavior checks selected from the independent stage-one coverage audit.
use pty_runtime_domain::terminal::program_status::{
    ProgramState, ProgramStatusKind, ProgramStatusStore,
};

fn feed(store: &mut ProgramStatusStore, bytes: &[u8]) {
    let plan = store.scan(bytes).unwrap();
    store.commit(plan).unwrap();
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let value = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            out.push(if i > chunk.len() {
                '='
            } else {
                ALPHABET[((value >> (18 - i * 6)) & 63) as usize] as char
            });
        }
    }
    out
}

#[test]
fn titles_preserve_original_text_and_display_invisible_characters_visibly() {
    // Include direction controls, joiners, variation selectors, shorthand/music
    // controls and tags; these are plain program text, never rendering commands.
    let codepoints = [
        0x00ad, 0x034f, 0x0600, 0x0605, 0x061c, 0x06dd, 0x070f, 0x0890, 0x0891, 0x08e2, 0x115f,
        0x1160, 0x17b4, 0x17b5, 0x180b, 0x180f, 0x200b, 0x200f, 0x202a, 0x202e, 0x2060, 0x206f,
        0x3164, 0xfe00, 0xfe0f, 0xfeff, 0xffa0, 0xfff9, 0xfffb, 0x110bd, 0x110cd, 0x13430, 0x1343f,
        0x1bca0, 0x1bca3, 0x1d173, 0x1d17a, 0xe0000, 0xe0fff,
    ];
    let mut store = ProgramStatusStore::default();
    for codepoint in codepoints {
        let character = char::from_u32(codepoint).unwrap();
        let original = format!("plain<{character}>text");
        let report = format!(
            "\x1b]7501;state=done:title={}\x07",
            base64(original.as_bytes())
        );
        feed(&mut store, report.as_bytes());
        let snapshot = store.records();
        let record = &snapshot.records[0];
        assert_eq!(record.title.as_deref(), Some(original.as_str()));
        assert_eq!(
            record.display_title().as_deref(),
            Some(format!("plain<\\u{{{codepoint:x}}}>text").as_str())
        );
        assert_eq!(record.display_msg(), None);
    }
    feed(&mut store, b"\x1b]7501;state=idle\x07");
    assert_eq!(store.records().records[0].display_title(), None);
}

#[test]
fn malformed_utf8_fields_follow_their_distinct_protocol_rules() {
    let mut store = ProgramStatusStore::default();
    feed(&mut store, b"\x1b]7501;state=done\x07");
    let before = store.records();
    feed(&mut store, b"\x1b]7501;state=error:id=\xff\x07");
    assert_eq!(store.records(), before, "invalid id cannot address root");

    feed(
        &mut store,
        b"\x1b]7501;\xff=ignored:state=blocked:kind=question:app=earlier:app=\xff:msg=b2s=\x07",
    );
    let snapshot = store.records();
    assert_eq!(snapshot.records.len(), 1);
    assert_eq!(snapshot.records[0].state, ProgramState::Blocked);
    assert_eq!(snapshot.records[0].kind, Some(ProgramStatusKind::Question));
    // Malformed pair syntax is skipped, preserving the earlier valid app.
    assert_eq!(snapshot.records[0].app.as_deref(), Some("earlier"));
    assert_eq!(snapshot.records[0].resolved_app.as_deref(), Some("earlier"));
    assert_eq!(snapshot.records[0].msg.as_deref(), Some("ok"));
}

#[test]
fn repeated_escape_preserves_query_detection_at_every_split() {
    let bytes = b"prefix\x1b\x1b]7501;?\x1b\\";
    for split in 0..=bytes.len() {
        let mut store = ProgramStatusStore::default();
        let first = store.scan(&bytes[..split]).unwrap();
        let mut offsets = first.query_offsets().to_vec();
        store.commit(first).unwrap();
        let second = store.scan(&bytes[split..]).unwrap();
        offsets.extend(second.query_offsets().iter().map(|offset| split + offset));
        assert_eq!(offsets, [bytes.len()]);
        store.commit(second).unwrap();
        assert!(store.records().records.is_empty());
    }
}

#[test]
fn clear_plan_retains_its_identifier_until_commit_without_early_mutation() {
    let mut store = ProgramStatusStore::default();
    feed(
        &mut store,
        b"\x1b]7501;state=done:id=parent/child\x07\x1b]7501;state=idle:id=other\x07",
    );
    let before = store.records();
    let bytes = b"\x1b]7501;state=clear:id=parent\x07";
    let plan = store.scan(bytes).unwrap();
    assert!(plan.retained_bytes() >= "parent".len());
    assert!(plan.retained_bytes() <= ProgramStatusStore::scan_reservation(bytes.len()).unwrap());
    assert_eq!(store.records(), before);
    store.commit(plan).unwrap();
    let after = store.records();
    assert_eq!(after.records.len(), 1);
    assert_eq!(after.records[0].id, "other");
    assert!(after.revision > before.revision);
}

#[test]
fn control_strings_hide_protocol_bytes_until_termination_or_cancellation() {
    for introducer in *b"PX^_" {
        for terminator in [b"\x1b\\".as_slice(), &[0x18], &[0x1a]] {
            let mut bytes = vec![27, introducer];
            bytes.extend_from_slice(b"hidden\x1bc\x1b]7501;state=error\x07");
            bytes.extend_from_slice(terminator);
            bytes.extend_from_slice(b"\x1b]7501;state=done:id=visible\x07");
            for split in 0..=bytes.len() {
                let mut store = ProgramStatusStore::default();
                feed(&mut store, b"\x1b]7501;state=idle:id=kept\x07");
                feed(&mut store, &bytes[..split]);
                feed(&mut store, &bytes[split..]);
                let snapshot = store.records();
                assert_eq!(snapshot.records.len(), 2);
                assert_eq!(snapshot.records[0].id, "kept");
                assert_eq!(snapshot.records[0].state, ProgramState::Idle);
                assert_eq!(snapshot.records[1].id, "visible");
                assert_eq!(snapshot.records[1].state, ProgramState::Done);
            }
        }
    }
}
