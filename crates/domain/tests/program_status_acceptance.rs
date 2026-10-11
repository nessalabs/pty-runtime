//! Independent OSC 7501 acceptance tests through the embedder-facing API.
use pty_runtime_domain::terminal::program_status::{
    ProgramState, ProgramStatusKind, ProgramStatusStore,
};

fn feed(store: &mut ProgramStatusStore, bytes: &[u8]) {
    let plan = store.scan(bytes).unwrap();
    store.commit(plan).unwrap();
}

fn report(store: &mut ProgramStatusStore, body: &str) {
    feed(store, format!("\x1b]7501;{body}\x1b\\").as_bytes());
}

#[test]
fn spec_brew_example_survives_shell_prompt_at_every_chunk_boundary() {
    let bytes = b"\x1b]7501;state=working:app=brew:msg=SW5zdGFsbGluZyB1cGRhdGVz\x1b\\\x1b]7501;state=blocked:kind=auth:app=brew:msg=UGFzc3dvcmQgcmVxdWlyZWQgdG8gaW5zdGFsbCB1cGRhdGVz\x07\x1b]7501;state=working:app=brew:msg=SW5zdGFsbGluZyB1cGRhdGVz\x1b\\\x1b]7501;state=done:app=brew:msg=VXBncmFkZWQgMTIgcGFja2FnZXM=\x1b\\\x1b]133;A\x07";
    for split in 0..=bytes.len() {
        let mut store = ProgramStatusStore::default();
        feed(&mut store, &bytes[..split]);
        feed(&mut store, &bytes[split..]);
        let snapshot = store.records();
        assert_eq!(snapshot.records.len(), 1, "split={split}");
        assert_eq!(snapshot.records[0].state, ProgramState::Done);
        assert_eq!(
            snapshot.records[0].msg.as_deref(),
            Some("Upgraded 12 packages")
        );
        assert_eq!(snapshot.records[0].resolved_app.as_deref(), Some("brew"));
    }
    let mut store = ProgramStatusStore::default();
    for byte in bytes {
        feed(&mut store, &[*byte]);
    }
    assert_eq!(store.records().records[0].state, ProgramState::Done);
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::new();
    for chunk in bytes.chunks(3) {
        let bits = ((chunk[0] as u32) << 16)
            | ((chunk.get(1).copied().unwrap_or(0) as u32) << 8)
            | chunk.get(2).copied().unwrap_or(0) as u32;
        for index in 0..4 {
            encoded.push(if index > chunk.len() {
                '='
            } else {
                ALPHABET[((bits >> (18 - index * 6)) & 63) as usize] as char
            });
        }
    }
    encoded
}

#[test]
fn invalid_duplicate_values_reject_whole_report_without_refreshing_eviction() {
    let bad_pairs = [
        "id=bad//id:id=old".to_owned(),
        "msg=AA==:msg=b2s=".to_owned(),
        "title=/w==:title=b2s=".to_owned(),
        format!("app={}:app=ok", "a".repeat(33)),
        format!("{}=x", "a".repeat(17)),
        format!("msg={}:msg=b2s=", base64(&[b'a'; 2049])),
        format!("title={}:title=b2s=", base64(&[b'a'; 193])),
    ];
    for bad in bad_pairs {
        let mut store = ProgramStatusStore::new(64).unwrap();
        report(&mut store, "state=done:id=old:msg=b2xk");
        for index in 1..64 {
            report(&mut store, &format!("state=idle:id=r{index}"));
        }
        let before = store.records();
        report(&mut store, &format!("state=error:id=old:{bad}"));
        assert_eq!(store.records(), before, "invalid pair {bad}");
        report(&mut store, "state=done:id=new");
        assert!(!store.records().records.iter().any(|r| r.id == "old"));
    }
}

#[test]
fn limits_accept_boundary_and_refuse_one_over_without_mutation() {
    let id128 = [
        "a".repeat(32),
        "b".repeat(32),
        "c".repeat(32),
        "d".repeat(29),
    ]
    .join("/");
    assert_eq!(id128.len(), 128);
    let cases = [
        (
            format!("{}=x", "a".repeat(16)),
            format!("{}=x", "a".repeat(17)),
        ),
        (
            format!("app={}", "a".repeat(32)),
            format!("app={}", "a".repeat(33)),
        ),
        (
            format!("id={}", "a".repeat(32)),
            format!("id={}", "a".repeat(33)),
        ),
        (format!("id={id128}"), format!("id={id128}e")),
        (
            "id=a/b/c/d/e/f/g/h".to_owned(),
            "id=a/b/c/d/e/f/g/h/i".to_owned(),
        ),
        (
            format!("msg={}", base64(&[b'a'; 2048])),
            format!("msg={}", base64(&[b'a'; 2049])),
        ),
        (
            format!("title={}", base64(&[b'a'; 192])),
            format!("title={}", base64(&[b'a'; 193])),
        ),
        (
            format!("msg={}", base64(&[b'a'; 2048])),
            format!("msg={}", "a".repeat(2733)),
        ),
        (
            format!("title={}", base64(&[b'a'; 192])),
            format!("title={}", "a".repeat(257)),
        ),
    ];
    for (index, (valid, invalid)) in cases.into_iter().enumerate() {
        let mut store = ProgramStatusStore::default();
        report(&mut store, &format!("state=done:{valid}"));
        assert_eq!(store.records().records.len(), 1, "boundary {index}");
        let before = store.records();
        report(&mut store, &format!("state=error:{invalid}"));
        assert_eq!(store.records(), before, "one over {index}");
    }
}

#[test]
fn sequence_limit_includes_both_terminator_bytes_and_recovers_after_oversize() {
    for terminator in ["\x07", "\x1b\\"] {
        for length in [4096, 4097] {
            let prefix = "\x1b]7501;state=done:unknown=";
            let sequence = format!(
                "{prefix}{}{terminator}",
                "a".repeat(length - prefix.len() - terminator.len())
            );
            let mut store = ProgramStatusStore::default();
            for chunk in sequence.as_bytes().chunks(13) {
                feed(&mut store, chunk);
            }
            assert_eq!(store.records().records.len(), usize::from(length == 4096));
            report(&mut store, "state=idle:id=next");
            assert!(store.records().records.iter().any(|r| r.id == "next"));
        }
    }
}

#[test]
fn malformed_pairs_are_skipped_but_known_fields_replace_instead_of_merge() {
    let mut store = ProgramStatusStore::default();
    report(
        &mut store,
        "state=blocked:kind=permission:progress=100:app=tool:msg=b2s=:title=b2s=",
    );
    report(
        &mut store,
        "badpair:=x:foo=bad!:state=bad: state = working :progress=0:unknown=value:app=bad,app",
    );
    let snapshot = store.records();
    let record = &snapshot.records[0];
    assert_eq!(record.state, ProgramState::Working);
    assert_eq!(record.progress, Some(0));
    assert!(
        record.app.is_none()
            && record.kind.is_none()
            && record.msg.is_none()
            && record.title.is_none()
    );
    let before = store.records();
    for ignored in ["id=other", "state=unknown", "state=clear:state=unknown"] {
        report(&mut store, ignored);
        assert_eq!(store.records(), before);
    }
    for progress in ["101", "-1", "1.0", "", "99999999999999999999999"] {
        report(
            &mut store,
            &format!("state=blocked:kind=invalid:progress={progress}"),
        );
        assert!(store.records().records[0].progress.is_none());
        assert!(store.records().records[0].kind.is_none());
    }
}

#[test]
fn inheritance_is_read_time_and_clear_respects_segment_boundaries() {
    let mut store = ProgramStatusStore::default();
    for body in [
        "state=working:app=deploy",
        "state=done:id=region/a",
        "state=error:id=region/b",
        "state=idle:id=regional",
        "state=blocked:id=region:app=local",
    ] {
        report(&mut store, body);
    }
    assert_eq!(
        store
            .records()
            .records
            .iter()
            .find(|r| r.id == "region/a")
            .unwrap()
            .resolved_app
            .as_deref(),
        Some("local")
    );
    report(&mut store, "state=done:id=region");
    assert_eq!(
        store
            .records()
            .records
            .iter()
            .find(|r| r.id == "region/a")
            .unwrap()
            .resolved_app
            .as_deref(),
        Some("deploy")
    );
    report(&mut store, "state=clear:id=region/a");
    assert!(store.records().records.iter().any(|r| r.id == "region"));
    report(&mut store, "state=clear:id=region");
    let ids: Vec<_> = store.records().records.into_iter().map(|r| r.id).collect();
    assert_eq!(ids, ["", "regional"]);
    report(&mut store, "state=clear");
    assert!(store.records().records.is_empty());
}

#[test]
fn lifecycle_markers_act_in_stream_order_and_other_osc_bodies_are_opaque() {
    let mut store = ProgramStatusStore::default();
    for state in ["idle", "working", "done", "blocked", "error"] {
        report(&mut store, &format!("state={state}:id={state}"));
    }
    let before = store.records();
    feed(
        &mut store,
        b"\x1b[!p\x1b[?1049h\x1b[?1049l\x1b]133;B\x07\x1b]133;AB\x07\x1b]0;title\x1bc\x07",
    );
    assert_eq!(store.records(), before);
    feed(
        &mut store,
        b"\x1b]133;A;extra\x07\x1b]7501;state=working:id=later\x07",
    );
    assert_eq!(store.records().records.len(), 4);
    store.process_exited().unwrap();
    assert_eq!(store.records().records.len(), 3);
    let before = store.records();
    store.process_exited().unwrap();
    assert_eq!(store.records(), before);
    feed(&mut store, b"\x1bc");
    assert!(store.records().records.is_empty());
}

#[test]
fn discarded_and_stale_plans_never_advance_scanner_or_records() {
    let mut store = ProgramStatusStore::default();
    let bytes = b"\x1b]7501;state=done\x07\x1b]7501;?\x1b\\tail";
    let abandoned = store.scan(bytes).unwrap();
    let retry = store.scan(bytes).unwrap();
    assert_eq!(abandoned.query_offsets(), retry.query_offsets());
    assert!(store.records().records.is_empty());
    store.commit(retry).unwrap();
    assert!(store.commit(abandoned).is_err());
    let other = ProgramStatusStore::default();
    assert!(store.commit(other.scan(b"\x1bc").unwrap()).is_err());
    let partial = store.scan(b"\x1b]7501;state=error").unwrap();
    drop(partial);
    feed(&mut store, b"\x07");
    assert_eq!(store.records().records[0].state, ProgramState::Done);
}

#[test]
fn query_offsets_end_at_terminators_even_when_prefix_is_in_previous_chunk() {
    let bytes = b"text\x1b]7501;?\x07\x1b]7501;?\x1b\\\x1b[c";
    // Derive positions from the actual delimiter bytes, independently of the scanner.
    let expected = [
        bytes.iter().position(|b| *b == 7).unwrap() + 1,
        bytes.iter().position(|b| *b == b'\\').unwrap() + 1,
    ];
    for split in 0..=bytes.len() {
        let mut store = ProgramStatusStore::default();
        let first = store.scan(&bytes[..split]).unwrap();
        let mut observed = first.query_offsets().to_vec();
        store.commit(first).unwrap();
        let second = store.scan(&bytes[split..]).unwrap();
        observed.extend(second.query_offsets().iter().map(|offset| offset + split));
        assert_eq!(observed, expected, "split={split}");
    }
}

#[test]
fn stored_text_is_plain_text_display_disarms_formatting_and_debug_is_redacted() {
    let text = "<b>secret</b>\u{202e}\u{2066}\u{200b}";
    let body = format!(
        "state=done:msg={}:title={}",
        base64(text.as_bytes()),
        base64(b"private-title")
    );
    let mut store = ProgramStatusStore::default();
    let bytes = format!("\x1b]7501;{body}\x07");
    let plan = store.scan(bytes.as_bytes()).unwrap();
    assert!(!format!("{plan:?}").contains("secret"));
    store.commit(plan).unwrap();
    let snapshot = store.records();
    assert_eq!(snapshot.records[0].msg.as_deref(), Some(text));
    let display = snapshot.records[0].display_msg().unwrap();
    assert!(!display.contains(['\u{202e}', '\u{2066}', '\u{200b}']));
    for debug in [
        format!("{store:?}"),
        format!("{snapshot:?}"),
        format!("{:?}", snapshot.records[0]),
    ] {
        assert!(!debug.contains("secret") && !debug.contains("private-title"));
    }
    for character in ['\0', '\u{1f}', '\u{7f}', '\u{80}', '\u{9f}'] {
        report(
            &mut store,
            &format!(
                "state=error:msg={}",
                base64(character.to_string().as_bytes())
            ),
        );
        assert_eq!(store.records(), snapshot);
    }
}

#[test]
fn saturation_evicts_oldest_update_and_replacement_does_not_evict() {
    assert!(ProgramStatusStore::new(63).is_err());
    assert!(ProgramStatusStore::new(257).is_err());
    let mut store = ProgramStatusStore::default();
    for id in 0..256 {
        report(&mut store, &format!("state=done:id=r{id}"));
    }
    report(&mut store, "state=idle:id=r0");
    assert_eq!(store.records().records.len(), 256);
    report(&mut store, "state=error:id=new");
    let snapshot = store.records();
    assert_eq!(snapshot.records.len(), 256);
    assert!(snapshot.records.iter().any(|r| r.id == "r0"));
    assert!(!snapshot.records.iter().any(|r| r.id == "r1"));
}

#[test]
fn spec_region_and_shell_examples_decode_and_inherit_under_bytewise_delivery() {
    let mut store = ProgramStatusStore::default();
    let examples = [
        (
            "state=working:app=deploy:msg=RGVwbG95aW5nIHYyLjQuMQ==",
            "",
            "Deploying v2.4.1",
        ),
        (
            "state=working:id=us-east:title=VVMgRWFzdA==:progress=40:msg=UHVzaGluZyBpbWFnZQ==",
            "us-east",
            "Pushing image",
        ),
        (
            "state=blocked:kind=permission:id=eu-west:title=RVUgV2VzdA==:msg=QXBwcm92ZSBkZXBsb3kgdG8gZXUtd2VzdCAocHJvZHVjdGlvbik/",
            "eu-west",
            "Approve deploy to eu-west (production)?",
        ),
        (
            "state=done:id=us-east:title=VVMgRWFzdA==:msg=SGVhbHRoeQ==",
            "us-east",
            "Healthy",
        ),
    ];
    for (body, id, text) in examples {
        for byte in format!("\x1b]7501;{body}\x1b\\").bytes() {
            feed(&mut store, &[byte]);
        }
        let snapshot = store.records();
        let record = snapshot.records.iter().find(|r| r.id == id).unwrap();
        assert_eq!(record.msg.as_deref(), Some(text));
        assert_eq!(record.resolved_app.as_deref(), Some("deploy"));
        if id == "eu-west" {
            assert_eq!(record.state, ProgramState::Blocked);
            assert_eq!(record.kind, Some(ProgramStatusKind::Permission));
            assert_eq!(
                snapshot
                    .records
                    .iter()
                    .find(|r| r.id.is_empty())
                    .unwrap()
                    .state,
                ProgramState::Working
            );
        }
    }
    for body in [
        "state=clear:id=us-east",
        "state=clear:id=eu-west",
        "state=done:app=deploy:msg=RGVwbG95ZWQgdG8gMyByZWdpb25z",
    ] {
        report(&mut store, body);
    }
    assert_eq!(store.records().records.len(), 1);
    assert_eq!(
        store.records().records[0].msg.as_deref(),
        Some("Deployed to 3 regions")
    );
    for (state, encoded, text) in [
        ("working", "U3luY2luZyBwaG90b3M=", "Syncing photos"),
        ("done", "UGhvdG9zIHN5bmNlZA==", "Photos synced"),
        ("error", "cnN5bmMgZmFpbGVk", "rsync failed"),
        (
            "blocked",
            "QXBwbHkgMyB0byBhZGQsIDEgdG8gY2hhbmdlLCAwIHRvIGRlc3Ryb3k/",
            "Apply 3 to add, 1 to change, 0 to destroy?",
        ),
    ] {
        report(
            &mut store,
            &format!("state={state}:kind=permission:app=terraform:msg={encoded}"),
        );
        assert_eq!(store.records().records[0].msg.as_deref(), Some(text));
    }
}

#[test]
fn invalid_ids_and_invalid_utf8_never_fall_back_to_root() {
    let mut store = ProgramStatusStore::default();
    report(&mut store, "state=done:msg=cHJvdGVjdGVk");
    let original = store.records();
    for id in ["/child", "child/", "a//b", "a,b", "a=b"] {
        report(&mut store, &format!("state=error:id={id}"));
        assert_eq!(store.records(), original, "id={id}");
    }
    for msg in ["/w==", "A", "A===", "AA=A", "AA==AA=="] {
        report(&mut store, &format!("state=error:msg={msg}"));
        assert_eq!(store.records(), original, "msg={msg}");
    }
}

#[test]
fn malformed_id_outside_value_alphabet_cannot_overwrite_root() {
    let mut store = ProgramStatusStore::default();
    report(&mut store, "state=done:msg=cHJvdGVjdGVk");
    let before = store.records();
    for id in ["bad!", "bad id", "bad\u{00e9}"] {
        report(&mut store, &format!("state=error:id={id}"));
        assert_eq!(store.records(), before, "invalid id {id}");
    }
}

#[test]
fn invalid_last_app_is_absent_instead_of_reviving_earlier_duplicate() {
    let mut store = ProgramStatusStore::default();
    for app in ["bad,app", "bad!", "bad app"] {
        report(&mut store, &format!("state=done:app=earlier:app={app}"));
        assert!(
            store.records().records[0].app.is_none(),
            "invalid app {app}"
        );
    }
}

#[test]
fn non_utf8_malformed_pair_does_not_discard_later_valid_pairs() {
    let mut store = ProgramStatusStore::default();
    feed(
        &mut store,
        b"\x1b]7501;unknown=\xff:state=done:msg=b2s=\x07",
    );
    let snapshot = store.records();
    assert_eq!(snapshot.records.len(), 1);
    assert_eq!(snapshot.records[0].msg.as_deref(), Some("ok"));
}

#[test]
fn advertised_reservations_cover_full_records_snapshots_and_dense_plans() {
    use pty_runtime_domain::terminal::program_status::MAX_SCAN_BYTES;
    for cap in [64, 65, 129, 256] {
        let mut store = ProgramStatusStore::new(cap).unwrap();
        let msg = base64(&[b'x'; 2048]);
        let title = base64(&[b'y'; 192]);
        for index in 0..cap {
            let id = format!(
                "{}/{}/{}/{index:029}",
                "a".repeat(32),
                "b".repeat(32),
                "c".repeat(32)
            );
            report(
                &mut store,
                &format!(
                    "state=done:id={id}:app={}:msg={msg}:title={title}",
                    "z".repeat(32)
                ),
            );
        }
        assert_eq!(store.records().records.len(), cap);
        assert!(store.retained_bytes() <= store.record_reservation());
        assert!(store.records().retained_bytes() <= store.record_reservation());
        let plan = store.scan(b"ordinary bytes").unwrap();
        // A scan of ordinary output must not retain copies of existing records.
        assert!(plan.retained_bytes() < 8192);
    }
    let store = ProgramStatusStore::default();
    for atom in [
        b"\x1b]7501;state=done\x07".as_slice(),
        b"\x1b]7501;?\x07",
        b"\x1bc",
    ] {
        let bytes = atom.repeat(MAX_SCAN_BYTES / atom.len());
        let plan = store.scan(&bytes).unwrap();
        assert!(
            plan.retained_bytes() <= ProgramStatusStore::scan_reservation(bytes.len()).unwrap()
        );
    }
    assert!(store.scan(&vec![b'x'; MAX_SCAN_BYTES + 1]).is_err());
    assert!(store.records().records.is_empty());
}
