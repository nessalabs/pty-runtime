use pty_runtime::event_stream::transport as events;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU8, Ordering},
};
use tokio::sync::Notify;
pub type Store = events::Runtime<events::infrastructure::MemoryStore>;
pub struct ControlledSink {
    pub store: Arc<Store>,
    pub mode: AtomicU8,
    pub calls: Mutex<Vec<events::NewEvent>>,
    pub entered: Notify,
    pub released: Notify,
}
impl ControlledSink {
    pub async fn new() -> Arc<Self> {
        let store = Store::open(
            events::infrastructure::MemoryStoreOptions::default(),
            events::RuntimeConfig::default(),
        )
        .await
        .unwrap();
        Arc::new(Self {
            store: Arc::new(store),
            mode: AtomicU8::new(0),
            calls: Mutex::new(Vec::new()),
            entered: Notify::new(),
            released: Notify::new(),
        })
    }
}
#[async_trait::async_trait]
impl events::EventSink for ControlledSink {
    async fn append(
        &self,
        stream: &events::StreamKey,
        event: events::NewEvent,
    ) -> events::Result<events::AppendReceipt> {
        self.calls.lock().unwrap().push(event.clone());
        let mode = self.mode.load(Ordering::Acquire);
        if mode == 1 {
            return Err(events::Error::CapacityExceeded);
        }
        if mode == 3 {
            self.entered.notify_one();
            self.released.notified().await;
        }
        let receipt = self.store.append(stream, event.clone()).await?;
        if mode == 2 {
            return Err(events::Error::CommitUnknown { event_id: event.id });
        }
        if mode == 4 {
            self.entered.notify_one();
            self.released.notified().await;
        }
        let mut record = (*receipt.record).clone();
        match mode {
            5 => record.cursor.version = 99,
            6 => record.cursor.offset = 0,
            7 => record.cursor.stream.incarnation.0[0] ^= 1,
            8 => record.event.payload = events::Payload::copy_from_slice(b"altered"),
            9 => record.cursor.offset = 1,
            _ => (),
        }
        Ok(events::AppendReceipt {
            record: Arc::new(record),
            kind: receipt.kind,
        })
    }
}
