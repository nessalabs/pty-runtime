//! Deterministic publication failures and cancellation against the real event store.
#![cfg(feature = "event-stream")]
#[path = "support/event_store.rs"]
mod event_store;
mod support;
use event_store::ControlledSink;
use events::{EventReader, EventRuntime};
use pty_runtime::{
    event_stream::{EventStreamPublisher, PublicationError, SinkFailure, transport as events},
    *,
};
use std::{sync::atomic::Ordering, time::Duration};

#[tokio::test]
async fn exact_pending_identity_survives_rejection_uncertain_commit_and_invalid_receipts() {
    tokio::time::timeout(Duration::from_secs(10), async {
        for mode in 1..=8 {
            if mode == 3 || mode == 4 {
                continue;
            }
            let owner = support::runtime(RuntimeOptions::default());
            let session = owner
                .spawn(
                    support::id("failure"),
                    &support::command("bytes", &["512"]),
                    support::options(),
                )
                .unwrap();
            support::block_on(session.wait().unwrap()).unwrap();
            let sink = ControlledSink::new().await;
            let stream = sink
                .store
                .create_stream(&events::StreamId::new("records").unwrap())
                .await
                .unwrap();
            let start = session.attach(AttachPosition::Oldest).unwrap();
            let before = start.cursor();
            let mut publisher =
                EventStreamPublisher::new(start, sink.clone(), stream.clone()).unwrap();
            sink.mode.store(mode, Ordering::Release);
            let error = publisher.publish_next().await.unwrap_err();
            assert!(matches!(
                error,
                PublicationError::Sink(_) | PublicationError::InvalidRecord
            ));
            let identity = publisher.pending_event_id().unwrap().clone();
            assert_eq!(publisher.acknowledged_byte_cursor(), before);
            assert!(publisher.publish_next().await.is_err());
            assert_eq!(publisher.pending_event_id(), Some(&identity));
            sink.mode.store(0, Ordering::Release);
            let published = publisher.publish_next().await.unwrap().unwrap();
            assert_eq!(published.byte_cursor.offset, 512);
            assert_eq!(published.event_cursor.offset, 1);
            {
                let calls = sink.calls.lock().unwrap();
                assert_eq!(calls.len(), 3);
                assert!(calls.iter().all(|event| *event == calls[0]));
            }
            assert_eq!(sink.store.bounds(&stream).await.unwrap().tail.offset, 1);
            sink.mode.store(1, Ordering::Release);
            assert_eq!(
                publisher.publish_next().await.unwrap_err(),
                PublicationError::Sink(SinkFailure::Capacity)
            );
            let completion_identity = publisher.pending_event_id().unwrap().clone();
            sink.mode.store(0, Ordering::Release);
            assert!(publisher.publish_next().await.unwrap().unwrap().complete);
            assert!(publisher.publish_next().await.unwrap().is_none());
            {
                let calls = sink.calls.lock().unwrap();
                assert_eq!(calls.len(), 5);
                assert_eq!(calls[3], calls[4]);
                assert_eq!(calls[4].id, completion_identity);
            }
            drop(publisher);
            assert!(
                sink.store
                    .shutdown(Duration::from_secs(2))
                    .await
                    .unwrap()
                    .closed
            );
            owner.shutdown();
        }
    })
    .await
    .expect("bounded publication failure test");
}

#[tokio::test]
async fn cancelled_sink_wait_retries_same_id_before_and_after_commit() {
    tokio::time::timeout(Duration::from_secs(10), async {
        for mode in [3, 4] {
            let owner = support::runtime(RuntimeOptions::default());
            let session = owner.spawn(support::id("cancel-wait"), &support::command("bytes", &["128"]), support::options()).unwrap();
            support::block_on(session.wait().unwrap()).unwrap();
            let sink = ControlledSink::new().await;
            let stream = sink.store.create_stream(&events::StreamId::new("records").unwrap()).await.unwrap();
            let attachment = session.attach(AttachPosition::Oldest).unwrap(); let before = attachment.cursor();
            let mut publisher = EventStreamPublisher::new(attachment, sink.clone(), stream.clone()).unwrap();
            sink.mode.store(mode, Ordering::Release);
            let mut wait = Box::pin(publisher.publish_next());
            tokio::select! { result = &mut wait => panic!("unexpected result {result:?}"), _ = sink.entered.notified() => () }
            drop(wait);
            assert_eq!(publisher.acknowledged_byte_cursor(), before);
            let identity = publisher.pending_event_id().unwrap().clone();
            assert_eq!(sink.store.bounds(&stream).await.unwrap().tail.offset, u64::from(mode == 4));
            sink.mode.store(0, Ordering::Release);
            assert_eq!(publisher.publish_next().await.unwrap().unwrap().event_cursor.offset, 1);
            { let calls = sink.calls.lock().unwrap(); assert_eq!(calls.len(), 2); assert_eq!(calls[0], calls[1]); assert_eq!(calls[0].id, identity); }
            drop(publisher); assert!(sink.store.shutdown(Duration::from_secs(2)).await.unwrap().closed);
            owner.shutdown();
        }
    }).await.expect("bounded cancelled publication test");
}

#[tokio::test]
async fn contradictory_store_cursor_cannot_acknowledge_a_different_completion_event() {
    let owner = support::runtime(RuntimeOptions::default());
    let session = owner
        .spawn(
            support::id("bad-offset"),
            &support::command("bytes", &["1"]),
            support::options(),
        )
        .unwrap();
    support::block_on(session.wait().unwrap()).unwrap();
    let sink = ControlledSink::new().await;
    let stream = sink
        .store
        .create_stream(&events::StreamId::new("records").unwrap())
        .await
        .unwrap();
    let mut publisher = EventStreamPublisher::new(
        session.attach(AttachPosition::Oldest).unwrap(),
        sink.clone(),
        stream,
    )
    .unwrap();
    assert_eq!(
        publisher
            .publish_next()
            .await
            .unwrap()
            .unwrap()
            .event_cursor
            .offset,
        1
    );
    sink.mode.store(9, Ordering::Release);
    assert_eq!(
        publisher.publish_next().await.unwrap_err(),
        PublicationError::InvalidRecord
    );
    assert!(publisher.pending_event_id().is_some());
    sink.mode.store(0, Ordering::Release);
    let completion = publisher.publish_next().await.unwrap().unwrap();
    assert!(completion.complete);
    assert_eq!(completion.event_cursor.offset, 2);
    drop(publisher);
    assert!(
        sink.store
            .shutdown(Duration::from_secs(2))
            .await
            .unwrap()
            .closed
    );
    owner.shutdown();
}
