//! Run with: cargo run --example event_stream_reconnect --features event-stream
use pty_runtime::{
    AttachPosition, CommandSpec, Runtime, RuntimeOptions, SessionId, SessionOptions, TerminalSize,
    event_stream::{EventStreamPublisher, decode_record, transport},
};
use std::{io, sync::Arc, time::Duration};
use transport::{EventReader, EventRuntime, EventSubscription};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cwd = std::env::current_dir()?;
    let pty = Runtime::new(vec![cwd.clone()], RuntimeOptions::default())?;
    let size = TerminalSize::new(80, 24).map_err(|_| io::Error::other("invalid terminal size"))?;
    let mut options = SessionOptions::raw(size);
    options.replay_bytes = 8; // Deliberately demonstrate an explicit lost-byte event.
    let command = CommandSpec::new(
        "/bin/sh".into(),
        cwd,
        vec!["-c".into(), "printf 0123456789abcdef".into()],
    )?;
    let id = SessionId::new("event-example".into())
        .map_err(|_| io::Error::other("invalid session id"))?;
    let session = pty.spawn(id, &command, options)?;
    let start = pty_runtime::ReplayCursor {
        lifetime: session.lifetime(),
        offset: 0,
    };
    session.wait()?.await?;
    let events = Arc::new(
        transport::Runtime::<transport::infrastructure::MemoryStore>::open(
            transport::infrastructure::MemoryStoreOptions::default(),
            transport::RuntimeConfig::default(),
        )
        .await?,
    );
    let stream = events
        .create_stream(&transport::StreamId::new("terminal-output")?)
        .await?;
    let observer = session.attach(AttachPosition::Cursor(start))?;
    let mut publisher = EventStreamPublisher::new(observer, events.clone(), stream.clone())?;
    let subscription_options = |start| transport::SubscriptionOptions {
        start,
        page: transport::PageLimits {
            max_records: 8,
            max_bytes: 1024 * 1024,
        },
        max_lag_records: 128,
        max_lag_duration: Duration::from_secs(30),
        catch_up_grace: Duration::from_secs(5),
    };
    publisher.publish_next().await?; // Publish the PTY replay gap.
    let mut first = events
        .subscribe(
            &stream,
            subscription_options(transport::StartPosition::Beginning),
        )
        .await?;
    let record = first
        .next()
        .await
        .ok_or_else(|| io::Error::other("subscription ended"))??;
    println!("first: {:?}", decode_record(&record)?);
    let reconnect_after = first.last_delivered().clone();
    drop(first);
    publisher.publish_next().await?; // Output accumulates while the subscriber is disconnected.
    let mut resumed = events
        .subscribe(
            &stream,
            subscription_options(transport::StartPosition::After(reconnect_after)),
        )
        .await?;
    let record = resumed
        .next()
        .await
        .ok_or_else(|| io::Error::other("reconnect ended"))??;
    println!("replayed: {:?}", decode_record(&record)?);
    publisher.publish_next().await?; // Completion arrives after reconnect, through the live path.
    let record = resumed
        .next()
        .await
        .ok_or_else(|| io::Error::other("live stream ended"))??;
    println!("live: {:?}", decode_record(&record)?);
    if publisher.publish_next().await?.is_some() {
        return Err(io::Error::other("duplicate completion publication").into());
    }
    drop(resumed);
    drop(publisher);
    let report = events.shutdown(Duration::from_secs(5)).await?;
    if !report.closed {
        return Err(io::Error::other("event store shutdown incomplete").into());
    }
    pty.shutdown();
    Ok(())
}
