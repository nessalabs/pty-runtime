use super::*;
fn feed(store: &mut ProgramStatusStore, bytes: &[u8]) {
    let plan = store.scan(bytes).unwrap();
    store.commit(plan).unwrap();
}
#[test]
fn control_strings_hide_embedded_commands_across_all_chunk_sizes() {
    for introducer in *b"]PX^_" {
        let mut bytes = vec![27, introducer];
        bytes.extend_from_slice(b"0;hidden\x1bc\x1b]7501;state=error\x1b\\");
        for chunk_size in 1..=bytes.len() {
            let mut store = ProgramStatusStore::default();
            feed(&mut store, b"\x1b]7501;state=done\x07");
            for chunk in bytes.chunks(chunk_size) {
                feed(&mut store, chunk)
            }
            assert_eq!(store.records().records[0].state, ProgramState::Done);
        }
    }
}
#[test]
fn cancelled_and_oversized_strings_recover_without_embedded_queries() {
    for cancel in [0x18, 0x1a] {
        let mut store = ProgramStatusStore::default();
        let mut bytes = b"\x1b]7501;state=working".to_vec();
        bytes.push(cancel);
        bytes.extend_from_slice(b"\x1b]7501;state=done\x07");
        feed(&mut store, &bytes);
        assert_eq!(store.records().records[0].state, ProgramState::Done);
    }
    let mut store = ProgramStatusStore::default();
    feed(&mut store, b"\x1b]7501;");
    for _ in 0..10 {
        feed(&mut store, &vec![b'x'; 4096]);
        assert!(store.retained_bytes() < 16384);
    }
    let plan = store.scan(b"\x1b]7501;?\x07").unwrap();
    assert!(plan.query_offsets().is_empty());
    store.commit(plan).unwrap();
    let plan = store.scan(b"\x1b]7501;?\x07").unwrap();
    assert_eq!(plan.query_offsets(), [9]);
}
#[test]
fn escape_before_bel_is_payload_not_a_query_terminator() {
    let store = ProgramStatusStore::default();
    assert!(
        store
            .scan(b"\x1b]7501;?\x1b\x07")
            .unwrap()
            .query_offsets()
            .is_empty()
    );
}
#[test]
fn reservation_bounds_cover_plans_store_and_snapshots() {
    let mut store = ProgramStatusStore::new(65).unwrap();
    for i in 0..128 {
        let bytes = format!(
            "\x1b]7501;state=working:id=r{i}:msg={}\x07",
            "eHh4".repeat(682)
        );
        let plan = store.scan(bytes.as_bytes()).unwrap();
        assert!(
            plan.retained_bytes() <= ProgramStatusStore::scan_reservation(bytes.len()).unwrap()
        );
        store.commit(plan).unwrap();
    }
    assert!(store.retained_bytes() <= store.record_reservation());
    assert!(store.records().retained_bytes() <= store.record_reservation());
    let bytes = b"\x1bc".repeat(4096);
    let plan = store.scan(&bytes).unwrap();
    assert!(plan.retained_bytes() <= ProgramStatusStore::scan_reservation(bytes.len()).unwrap());
    assert_eq!(
        ProgramStatusStore::scan_reservation(MAX_SCAN_BYTES + 1),
        Err(ProgramStatusError::ChunkTooLarge)
    );
}

#[test]
fn state_only_report_plan_counts_its_boxed_record_allocation() {
    let store = ProgramStatusStore::default();
    let plan = store.scan(b"\x1b]7501;state=done\x07").unwrap();
    // The record is boxed even when all its strings have zero capacity.
    assert!(plan.retained_bytes() >= std::mem::size_of::<ProgramStatusRecord>());
}

#[test]
fn allocation_admission_follows_committed_continuation_and_snapshot_contents() {
    let mut store = ProgramStatusStore::default();
    assert_eq!(store.scan_reservation_for(b"ordinary output").unwrap(), 0);
    assert_eq!(store.snapshot_reservation(), 0);
    feed(&mut store, b"\x1b");
    assert!(store.scan_reservation_for(b"]7501;state=done\x07").unwrap() > 0);
    feed(&mut store, b"]7501;state=done\x07");
    assert_eq!(store.scan_reservation_for(b"ordinary output").unwrap(), 0);
    assert!(store.records().retained_bytes() <= store.snapshot_reservation());
    assert_eq!(
        store.scan_reservation_for(&vec![b'x'; MAX_SCAN_BYTES + 1]),
        Err(ProgramStatusError::ChunkTooLarge)
    );
}
