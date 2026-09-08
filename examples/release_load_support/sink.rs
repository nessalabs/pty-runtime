use super::Result;
use pty_runtime::Session;
#[cfg(feature = "event-stream")]
mod enabled {
    use super::*;
    use events::{EventReader, EventSink};
    use pty_runtime::{
        AttachPosition,
        event_stream::{EventStreamPublisher, transport as events},
    };
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };
    #[derive(Default)]
    struct NeverSink {
        calls: AtomicU64,
        bytes: AtomicU64,
        inflight: AtomicU64,
    }
    struct Retained<'a>(&'a NeverSink);
    impl Drop for Retained<'_> {
        fn drop(&mut self) {
            self.0.inflight.fetch_sub(1, Ordering::Relaxed);
        }
    }
    #[async_trait::async_trait]
    impl EventSink for NeverSink {
        async fn append(
            &self,
            _: &events::StreamKey,
            event: events::NewEvent,
        ) -> events::Result<events::AppendReceipt> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_max(event.payload.len() as u64, Ordering::Relaxed);
            self.inflight.fetch_add(1, Ordering::Relaxed);
            let _retained = Retained(self);
            let result = std::future::pending().await;
            drop(event);
            result
        }
    }
    pub struct StalledSink {
        task: tokio::task::JoinHandle<()>,
        sink: Arc<NeverSink>,
    }
    impl StalledSink {
        pub async fn start(session: &Session) -> Result<Self> {
            let store = events::Runtime::<events::infrastructure::MemoryStore>::open(
                events::infrastructure::MemoryStoreOptions::default(),
                events::RuntimeConfig::default(),
            )
            .await?;
            let stream = store
                .create_stream(&events::StreamId::new("load-stall")?)
                .await?;
            let sink = Arc::new(NeverSink::default());
            let mut publisher = EventStreamPublisher::new(
                session.attach(AttachPosition::Tail)?,
                sink.clone(),
                stream,
            )?;
            let task = tokio::spawn(async move {
                let _ = publisher.publish_next().await;
                drop(store);
            });
            Ok(Self { task, sink })
        }
        pub fn report(&self) {
            let calls = self.sink.calls.load(Ordering::Relaxed);
            let inflight = self.sink.inflight.load(Ordering::Relaxed);
            let bytes = self.sink.bytes.load(Ordering::Relaxed);
            assert_eq!(calls, 1);
            assert_eq!(inflight, 1);
            assert!(bytes <= 17000);
            println!(
                "{{\"event\":\"stalled_sink\",\"calls\":{calls},\"inflight\":{inflight},\"max_payload_bytes\":{bytes}}}"
            );
        }
        pub async fn stop(self) -> Result<()> {
            self.report();
            self.task.abort();
            assert!(self.task.await.unwrap_err().is_cancelled());
            assert_eq!(self.sink.inflight.load(Ordering::Relaxed), 0);
            Ok(())
        }
    }
}
#[cfg(feature = "event-stream")]
pub use enabled::StalledSink;
#[cfg(not(feature = "event-stream"))]
pub struct StalledSink;
#[cfg(not(feature = "event-stream"))]
impl StalledSink {
    pub async fn start(_: &Session) -> Result<Self> {
        Err(std::io::Error::other("stalled-sink requires --features event-stream").into())
    }
    pub fn report(&self) {}
    pub async fn stop(self) -> Result<()> {
        Ok(())
    }
}
