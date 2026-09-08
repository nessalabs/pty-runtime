//! Actual pinned event store publication, replay and live reconnect.
#![cfg(feature = "event-stream")]
mod support;
use events::{EventReader, EventRuntime, EventSink, EventSubscription};
use pty_runtime::{
    event_stream::{EventStreamPublisher, decode_record, transport as events},
    *,
};
use std::{sync::Arc, time::Duration};

fn subscription(start: events::StartPosition) -> events::SubscriptionOptions {
    events::SubscriptionOptions {
        start,
        page: events::PageLimits {
            max_records: 8,
            max_bytes: 1024 * 1024,
        },
        max_lag_records: 100,
        max_lag_duration: Duration::from_secs(10),
        catch_up_grace: Duration::from_secs(5),
    }
}
#[tokio::test]
async fn real_store_subscription_reconnect_and_interleaved_writer_keep_distinct_cursors() {
    let owner = support::runtime(RuntimeOptions::default());
    let mut options = support::options();
    options.replay_bytes = 31;
    let session = owner
        .spawn(
            support::id("event-reconnect"),
            &support::command("bytes", &["10000"]),
            options,
        )
        .unwrap();
    let start = ReplayCursor {
        lifetime: session.lifetime(),
        offset: 0,
    };
    let completion = support::block_on(session.wait().unwrap()).unwrap();
    let store = Arc::new(
        events::Runtime::<events::infrastructure::MemoryStore>::open(
            events::infrastructure::MemoryStoreOptions::default(),
            events::RuntimeConfig::default(),
        )
        .await
        .unwrap(),
    );
    let stream = store
        .create_stream(&events::StreamId::new("pty-output").unwrap())
        .await
        .unwrap();
    let attachment = session.attach(AttachPosition::Cursor(start)).unwrap();
    let mut publisher =
        EventStreamPublisher::new(attachment, store.clone(), stream.clone()).unwrap();
    let first = publisher.publish_next().await.unwrap().unwrap();
    assert_eq!(first.event_cursor.offset, 1);
    assert_eq!(first.byte_cursor.offset, 9969);
    let mut subscriber = store
        .subscribe(&stream, subscription(events::StartPosition::Beginning))
        .await
        .unwrap();
    let gap = subscriber.next().await.unwrap().unwrap();
    assert!(
        matches!(decode_record(&gap).unwrap().output, OutputEvent::Replay(ReplayPage::Gap {from,to}) if from==start && to.offset==9969)
    );
    let resume = subscriber.last_delivered().clone();
    drop(subscriber);
    store
        .append(
            &stream,
            events::NewEvent {
                id: events::EventId::new("other-writer").unwrap(),
                schema: events::SchemaRef {
                    id: events::SchemaId::new("unrelated").unwrap(),
                    version: 1,
                },
                payload: events::Payload::copy_from_slice(b"metadata"),
            },
        )
        .await
        .unwrap();
    let bytes = publisher.publish_next().await.unwrap().unwrap();
    assert_eq!(bytes.event_cursor.offset, 3);
    assert_eq!(bytes.byte_cursor.offset, 10000);
    let mut subscriber = store
        .subscribe(&stream, subscription(events::StartPosition::After(resume)))
        .await
        .unwrap();
    let unrelated = subscriber.next().await.unwrap().unwrap();
    assert!(decode_record(&unrelated).is_err());
    let replayed = subscriber.next().await.unwrap().unwrap();
    assert_eq!(replayed.cursor, bytes.event_cursor);
    assert!(
        matches!(decode_record(&replayed).unwrap().output, OutputEvent::Replay(ReplayPage::Bytes {bytes,..}) if bytes==(9969..10000).map(|n|(n%251) as u8).collect::<Vec<_>>())
    );
    let live = publisher.publish_next().await.unwrap().unwrap();
    assert!(live.complete);
    let delivered = subscriber.next().await.unwrap().unwrap();
    assert_eq!(delivered.cursor, live.event_cursor);
    assert_eq!(
        decode_record(&delivered).unwrap().output,
        OutputEvent::Complete(completion)
    );
    assert!(publisher.publish_next().await.unwrap().is_none());
    assert_eq!(store.bounds(&stream).await.unwrap().tail.offset, 4);
    drop(subscriber);
    drop(publisher);
    assert!(store.shutdown(Duration::from_secs(5)).await.unwrap().closed);
    owner.shutdown();
}

#[tokio::test]
async fn actual_memory_store_full_and_stale_incarnation_preserve_pending_without_eviction() {
    let owner = support::runtime(RuntimeOptions::default());
    let session = owner
        .spawn(
            support::id("store-full"),
            &support::command("bytes", &["64"]),
            support::options(),
        )
        .unwrap();
    support::block_on(session.wait().unwrap()).unwrap();
    let store = Arc::new(
        events::Runtime::<events::infrastructure::MemoryStore>::open(
            events::infrastructure::MemoryStoreOptions {
                max_history_records: 1,
                ..Default::default()
            },
            events::RuntimeConfig::default(),
        )
        .await
        .unwrap(),
    );
    let stream = store
        .create_stream(&events::StreamId::new("one-record").unwrap())
        .await
        .unwrap();
    let mut stale = stream.clone();
    stale.incarnation.0[0] ^= 1;
    let mut publisher = EventStreamPublisher::new(
        session.attach(AttachPosition::Oldest).unwrap(),
        store.clone(),
        stale,
    )
    .unwrap();
    assert_eq!(
        publisher.publish_next().await.unwrap_err(),
        event_stream::PublicationError::Sink(event_stream::SinkFailure::Unavailable)
    );
    assert_eq!(publisher.acknowledged_byte_cursor().offset, 0);
    assert!(publisher.pending_event_id().is_some());
    assert_eq!(store.bounds(&stream).await.unwrap().tail.offset, 0);
    drop(publisher);
    let mut publisher = EventStreamPublisher::new(
        session.attach(AttachPosition::Oldest).unwrap(),
        store.clone(),
        stream.clone(),
    )
    .unwrap();
    let output = publisher.publish_next().await.unwrap().unwrap();
    assert_eq!(output.byte_cursor.offset, 64);
    let before = store.bounds(&stream).await.unwrap();
    for _ in 0..3 {
        assert_eq!(
            publisher.publish_next().await.unwrap_err(),
            event_stream::PublicationError::Sink(event_stream::SinkFailure::Capacity)
        );
        assert_eq!(publisher.acknowledged_byte_cursor(), output.byte_cursor);
    }
    assert_eq!(store.bounds(&stream).await.unwrap(), before);
    let retained = store
        .read_after(
            &events::Cursor::new(stream, 0),
            events::PageLimits {
                max_records: 1,
                max_bytes: 1024 * 1024,
            },
            None,
        )
        .await
        .unwrap();
    assert!(
        matches!(decode_record(&retained.records[0]).unwrap().output, OutputEvent::Replay(ReplayPage::Bytes {bytes,..}) if bytes.len()==64)
    );
    drop(publisher);
    assert!(store.shutdown(Duration::from_secs(2)).await.unwrap().closed);
    owner.shutdown();
}
