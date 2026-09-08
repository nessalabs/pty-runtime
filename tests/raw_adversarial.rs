//! Independently polled asynchronous wait cancellation and quota ownership.
mod support;
use pty_runtime::*;
use std::{
    future::Future,
    task::{Context, Poll, Waker},
};
use support::*;

#[test]
fn dropping_polled_pending_waits_releases_admission_and_preserves_cursor() {
    let owner = runtime(RuntimeOptions {
        max_observers: 1,
        ..RuntimeOptions::default()
    });
    let session = owner
        .spawn(id("polled-wait"), &command("echo", &[]), options())
        .unwrap();
    let mut observer = session.attach(AttachPosition::Oldest).unwrap();
    assert!(
        matches!(block_on(observer.read_next()).unwrap(), OutputEvent::Replay(ReplayPage::Bytes { bytes, .. }) if bytes == b"ready")
    );
    let before = observer.cursor();
    let waker = Waker::noop();
    let mut cx = Context::from_waker(waker);
    {
        let mut pending = std::pin::pin!(observer.read_next());
        assert!(matches!(pending.as_mut().poll(&mut cx), Poll::Pending));
    }
    assert_eq!(observer.cursor(), before);
    drop(observer);
    {
        let mut pending = std::pin::pin!(session.wait().unwrap());
        assert!(matches!(pending.as_mut().poll(&mut cx), Poll::Pending));
    }
    let mut observer = session.attach(AttachPosition::Cursor(before)).unwrap();
    assert_eq!(block_on(session.write(b"test").unwrap()).written, 4);
    let mut received = Vec::new();
    while received.len() < 4 {
        if let OutputEvent::Replay(ReplayPage::Bytes { bytes, .. }) =
            block_on(observer.read_next()).unwrap()
        {
            received.extend(bytes);
        }
    }
    assert_eq!(received, b"test");
    drop(observer);
    session.cancel().unwrap();
    assert!(
        block_on(session.wait().unwrap())
            .unwrap()
            .status
            .exit
            .is_some()
    );
}
