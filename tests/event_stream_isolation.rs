//! External sink backpressure and observer cancellation do not own PTY progress.
#![cfg(feature = "event-stream")]
use pty_runtime::terminal::ControlGeneration;
#[path = "support/event_store.rs"]
mod event_store;
mod support;
use event_store::ControlledSink;
use events::{EventReader, EventRuntime};
use pty_runtime::{
    event_stream::{EventStreamPublisher, transport as events},
    *,
};
use std::{future::Future, sync::atomic::Ordering, task::Poll, time::Duration};

struct OwnsAttachment {
    _attachment: Attachment,
}
// The owned attachment's destructor is the reentrant behavior under test.
#[allow(clippy::manual_noop_waker)]
impl std::task::Wake for OwnsAttachment {
    fn wake(self: std::sync::Arc<Self>) {}
}
struct CountOrPanicWake {
    count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    panic: bool,
}
impl std::task::Wake for CountOrPanicWake {
    fn wake(self: std::sync::Arc<Self>) {
        self.count.fetch_add(1, Ordering::Release);
        assert!(!self.panic, "injected observer wake panic");
    }
}

#[tokio::test]
async fn panicking_publisher_waker_does_not_suppress_another_observer_or_pty_output() {
    tokio::time::timeout(Duration::from_secs(8), async {
        let owner = support::runtime(RuntimeOptions::default());
        let session = owner
            .spawn(
                support::id("wake-panic"),
                &support::command("echo", &[]),
                support::options(),
            )
            .unwrap();
        let mut ready = session.attach(AttachPosition::Oldest).unwrap();
        support::block_on(ready.read_next()).unwrap();
        drop(ready);
        let sink = ControlledSink::new().await;
        let stream = sink
            .store
            .create_stream(&events::StreamId::new("wake-panic").unwrap())
            .await
            .unwrap();
        let mut first = EventStreamPublisher::new(
            session.attach(AttachPosition::Tail).unwrap(),
            sink.clone(),
            stream.clone(),
        )
        .unwrap();
        let mut second = EventStreamPublisher::new(
            session.attach(AttachPosition::Tail).unwrap(),
            sink.clone(),
            stream,
        )
        .unwrap();
        let bad_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let good_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let bad = std::task::Waker::from(std::sync::Arc::new(CountOrPanicWake {
            count: bad_count.clone(),
            panic: true,
        }));
        let good = std::task::Waker::from(std::sync::Arc::new(CountOrPanicWake {
            count: good_count.clone(),
            panic: false,
        }));
        let mut bad_wait = Box::pin(first.publish_next());
        let mut good_wait = Box::pin(second.publish_next());
        assert!(
            bad_wait
                .as_mut()
                .poll(&mut std::task::Context::from_waker(&bad))
                .is_pending()
        );
        assert!(
            good_wait
                .as_mut()
                .poll(&mut std::task::Context::from_waker(&good))
                .is_pending()
        );
        assert_eq!(session.write(b"survives").unwrap().await.written, 8);
        while good_count.load(Ordering::Acquire) == 0 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        assert_eq!(bad_count.load(Ordering::Acquire), 1);
        drop(bad_wait);
        drop(good_wait);
        assert_eq!(
            second
                .publish_next()
                .await
                .unwrap()
                .unwrap()
                .byte_cursor
                .offset,
            13
        );
        assert_eq!(session.write(b"again").unwrap().await.written, 5);
        assert_eq!(
            second
                .publish_next()
                .await
                .unwrap()
                .unwrap()
                .byte_cursor
                .offset,
            18
        );
        drop(first);
        drop(second);
        session.cancel().unwrap();
        session.wait().unwrap().await.unwrap();
        assert!(
            sink.store
                .shutdown(Duration::from_secs(2))
                .await
                .unwrap()
                .closed
        );
        owner.shutdown();
    })
    .await
    .expect("observer panic isolation deadline");
}

#[tokio::test]
async fn cancelled_publication_drops_a_reentrant_source_waker_without_locking_the_session() {
    let owner = support::runtime(RuntimeOptions::default());
    let session = owner
        .spawn(
            support::id("source-waker-drop"),
            &support::command("echo", &[]),
            support::options(),
        )
        .unwrap();
    let mut ready = session.attach(AttachPosition::Oldest).unwrap();
    support::block_on(ready.read_next()).unwrap();
    drop(ready);
    let sink = ControlledSink::new().await;
    let stream = sink
        .store
        .create_stream(&events::StreamId::new("source-waker").unwrap())
        .await
        .unwrap();
    let first = session.attach(AttachPosition::Tail).unwrap();
    let second = session.attach(AttachPosition::Tail).unwrap();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut publisher = EventStreamPublisher::new(first, sink, stream).unwrap();
        let waker = std::task::Waker::from(std::sync::Arc::new(OwnsAttachment {
            _attachment: second,
        }));
        let mut wait = Box::pin(publisher.publish_next());
        assert!(
            wait.as_mut()
                .poll(&mut std::task::Context::from_waker(&waker))
                .is_pending()
        );
        drop(waker);
        // The source registration owns the last waker; its destructor detaches
        // another observer from exactly the same session state.
        drop(wait);
        drop(publisher);
        session.cancel().unwrap();
        support::block_on(session.wait().unwrap()).unwrap();
        owner.shutdown();
        sender.send(()).unwrap();
    });
    assert_eq!(receiver.recv_timeout(Duration::from_secs(3)), Ok(()));
}

#[tokio::test]
async fn cancelling_before_source_read_keeps_cursor_and_observer_reusable() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let owner = support::runtime(RuntimeOptions::default());
        let session = owner
            .spawn(
                support::id("read-cancel"),
                &support::command("echo", &[]),
                support::options(),
            )
            .unwrap();
        let mut ready = session.attach(AttachPosition::Oldest).unwrap();
        support::block_on(ready.read_next()).unwrap();
        drop(ready);
        let sink = ControlledSink::new().await;
        let stream = sink
            .store
            .create_stream(&events::StreamId::new("output").unwrap())
            .await
            .unwrap();
        let attachment = session.attach(AttachPosition::Tail).unwrap();
        let initial = attachment.cursor();
        let mut publisher = EventStreamPublisher::new(attachment, sink.clone(), stream).unwrap();
        let mut pending = Box::pin(publisher.publish_next());
        std::future::poll_fn(|cx| {
            assert!(pending.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
        drop(pending);
        assert!(publisher.pending_event_id().is_none());
        assert_eq!(publisher.acknowledged_byte_cursor(), initial);
        assert!(sink.calls.lock().unwrap().is_empty());
        assert_eq!(session.write(b"still-alive").unwrap().await.written, 11);
        let receipt = publisher.publish_next().await.unwrap().unwrap();
        assert_eq!(receipt.byte_cursor.offset, initial.offset + 11);
        drop(publisher);
        session.cancel().unwrap();
        session.wait().unwrap().await.unwrap();
        assert!(
            sink.store
                .shutdown(Duration::from_secs(2))
                .await
                .unwrap()
                .closed
        );
        owner.shutdown();
    })
    .await
    .expect("source read cancellation deadline");
}

#[cfg(feature = "ghostty")]
#[tokio::test]
async fn stalled_sink_does_not_block_parser_input_resize_or_cancel_and_later_reports_gap() {
    use pty_runtime::event_stream::decode_record;
    tokio::time::timeout(Duration::from_secs(15), async {
        let owner=support::runtime(RuntimeOptions::default());
        let mut options=SessionOptions::projected(terminal::TerminalConfig::new(TerminalSize::new(80,24).unwrap()));
        options.replay_bytes=31;
        let session=owner.spawn(support::id("sink-stall"),&support::command("echo",&[]),options).unwrap();
        let sink=ControlledSink::new().await;
        let stream=sink.store.create_stream(&events::StreamId::new("output").unwrap()).await.unwrap();
        let mut publisher=EventStreamPublisher::new(session.attach(AttachPosition::Oldest).unwrap(),sink.clone(),stream.clone()).unwrap();
        sink.mode.store(3,Ordering::Release);
        let mut pending=Box::pin(publisher.publish_next());
        tokio::select! { result=&mut pending=>panic!("sink did not stall: {result:?}"), _=sink.entered.notified()=>() }
        // Sink is stalled with the original ready record retained. Runtime work must progress.
        let resize=session.resize_projected(TerminalSize::new(100,30).unwrap()).unwrap().await.unwrap();
        assert!(resize.os.is_ok()); assert!(resize.model.is_ok());
        for _ in 0..64 { let outcome=session.write(&[b'z';256]).unwrap().await; assert_eq!(outcome.written,256); assert!(outcome.error.is_none()); }
        let expected=5+64*256;
        loop {
            let status=session.projection_status().unwrap().unwrap();
            if status.processed.offset>=expected { break; }
            assert!(status.failure.is_none()); tokio::time::sleep(Duration::from_millis(1)).await;
        }
        let view=session.projected_view().unwrap().await.unwrap(); assert!(view.processed().offset>=expected); assert_eq!(view.control_generation(),ControlGeneration::from_raw(1));
        session.cancel().unwrap(); let completion=session.wait().unwrap().await.unwrap(); assert!(completion.status.exit.is_some());
        drop(pending);
        assert_eq!(publisher.acknowledged_byte_cursor().offset,0);
        sink.mode.store(0,Ordering::Release);
        let first=publisher.publish_next().await.unwrap().unwrap(); assert_eq!(first.byte_cursor.offset,5);
        let gap=publisher.publish_next().await.unwrap().unwrap(); assert!(gap.byte_cursor.offset>first.byte_cursor.offset);
        let page=sink.store.read_after(&first.event_cursor,events::PageLimits { max_records:1,max_bytes:1024*1024 },Some(&gap.event_cursor)).await.unwrap();
        assert!(matches!(decode_record(&page.records[0]).unwrap().output,OutputEvent::Replay(ReplayPage::Gap {from,to}) if from.offset==5 && to.offset==expected-31));
        drop(publisher); assert!(sink.store.shutdown(Duration::from_secs(2)).await.unwrap().closed); owner.shutdown();
    }).await.expect("sink stall isolation deadline");
}
