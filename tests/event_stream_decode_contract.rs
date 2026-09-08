//! Untrusted records must fail before becoming portable PTY observations.
#![cfg(feature = "event-stream")]
mod support;
use events::{EventReader, EventRuntime};
use pty_runtime::{
    event_stream::{EventStreamPublisher, PublicationError, decode_record, transport as events},
    *,
};
use std::{sync::Arc, time::Duration};

async fn records() -> Vec<Arc<events::Record>> {
    let owner = support::runtime(RuntimeOptions::default());
    let mut options = support::options();
    options.replay_bytes = 4;
    let session = owner
        .spawn(
            support::id("decode-boundary"),
            &support::command("bytes", &["8"]),
            options,
        )
        .unwrap();
    support::block_on(session.wait().unwrap()).unwrap();
    let store = Arc::new(
        events::Runtime::<events::infrastructure::MemoryStore>::open(
            events::infrastructure::MemoryStoreOptions::default(),
            events::RuntimeConfig::default(),
        )
        .await
        .unwrap(),
    );
    let stream = store
        .create_stream(&events::StreamId::new("decode-contract").unwrap())
        .await
        .unwrap();
    let attachment = session
        .attach(AttachPosition::Cursor(ReplayCursor {
            lifetime: session.lifetime(),
            offset: 0,
        }))
        .unwrap();
    let mut publisher =
        EventStreamPublisher::new(attachment, store.clone(), stream.clone()).unwrap();
    while publisher.publish_next().await.unwrap().is_some() {}
    let records = store
        .read_after(
            &events::Cursor::new(stream, 0),
            events::PageLimits {
                max_records: 8,
                max_bytes: 1024 * 1024,
            },
            None,
        )
        .await
        .unwrap()
        .records;
    assert_eq!(records.len(), 3);
    assert!(matches!(
        decode_record(&records[0]).unwrap().output,
        OutputEvent::Replay(ReplayPage::Gap { .. })
    ));
    assert!(matches!(
        decode_record(&records[1]).unwrap().output,
        OutputEvent::Replay(ReplayPage::Bytes { .. })
    ));
    assert!(matches!(
        decode_record(&records[2]).unwrap().output,
        OutputEvent::Complete(_)
    ));
    drop(publisher);
    assert!(store.shutdown(Duration::from_secs(2)).await.unwrap().closed);
    owner.shutdown();
    records
}

fn payload(record: &events::Record, bytes: &[u8]) -> events::Record {
    let mut changed = record.clone();
    changed.event.payload = events::Payload::copy_from_slice(bytes);
    changed
}

fn rejects(record: &events::Record, label: &str) {
    assert_eq!(
        decode_record(record),
        Err(PublicationError::InvalidRecord),
        "{label}"
    );
}

#[tokio::test]
async fn every_truncation_trailing_byte_and_invalid_envelope_is_rejected() {
    for (kind, record) in records().await.iter().enumerate() {
        let bytes = record.event.payload.as_bytes();
        for end in 0..bytes.len() {
            rejects(
                &payload(record, &bytes[..end]),
                &format!("record {kind} truncated at {end}"),
            );
        }
        let mut appended = bytes.to_vec();
        appended.push(0);
        rejects(&payload(record, &appended), "trailing byte");
        rejects(&payload(record, &vec![0; 65536 + 65]), "oversized record");
        let mut changed = record.as_ref().clone();
        changed.event.schema.version += 1;
        rejects(&changed, "unknown schema version");
        changed = record.as_ref().clone();
        changed.event.schema.id = events::SchemaId::new("foreign-schema").unwrap();
        rejects(&changed, "foreign schema");
        changed = record.as_ref().clone();
        changed.cursor.version += 1;
        rejects(&changed, "unknown cursor version");
        changed = record.as_ref().clone();
        changed.cursor.offset = 0;
        rejects(&changed, "non-record cursor");
        for (at, value) in [(0, b'X'), (4, 0), (4, 255)] {
            let mut corrupt = bytes.to_vec();
            corrupt[at] = value;
            rejects(&payload(record, &corrupt), "magic or output kind");
        }
    }
}

#[tokio::test]
async fn contradictory_ranges_and_incomplete_or_unknown_completion_fields_are_rejected() {
    let records = records().await;
    for record in &records[..2] {
        let bytes = record.event.payload.as_bytes();
        for next in [0_u64, 4] {
            let mut corrupt = bytes.to_vec();
            // Version-one header: magic, kind, owner, lifetime, from, next.
            corrupt[29..37].copy_from_slice(&next.to_le_bytes());
            if corrupt != bytes {
                rejects(&payload(record, &corrupt), "impossible output range");
            }
        }
        let mut backwards = bytes.to_vec();
        backwards[21..29].copy_from_slice(&u64::MAX.to_le_bytes());
        rejects(&payload(record, &backwards), "backward range");
    }
    let complete = &records[2];
    let bytes = complete.event.payload.as_bytes();
    // Completion fields: exit tag/i32, drain tag/u16, supervision/admission u16, bool.
    for at in [37, 42, 44, 45, 47, 49] {
        let mut corrupt = bytes.to_vec();
        corrupt[at] = 255;
        rejects(&payload(complete, &corrupt), "unknown completion field");
    }
    let mut incomplete = bytes.to_vec();
    incomplete[37..].fill(0);
    rejects(&payload(complete, &incomplete), "no completion facts");
    let mut contradictory = bytes.to_vec();
    contradictory[29..37].copy_from_slice(&9_u64.to_le_bytes());
    rejects(
        &payload(complete, &contradictory),
        "completion must not advance byte cursor",
    );
}
