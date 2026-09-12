//! ADR 0003: an unrestorable checkpoint must not take the session with it.
//!
//! > If a committed checkpoint cannot be restored, report projection
//! > unavailability and preserve process ownership and bounded raw I/O.
//!
//! The second half is a cross-layer claim about a real child: its PTY, its
//! reader and writer, and its cancellation must all keep working after the
//! projection is gone. A mock process cannot show that — a dummy `IProcessSession`
//! returns a constant pid and succeeds at everything by construction, so it
//! passes whether or not the real child survived. This test therefore uses the
//! real `UnixProcessBackend` and a real child, and only the checkpoint store is
//! substituted, so that a parked session can be made unrestorable on demand.
#![cfg(feature = "ghostty")]
#[allow(dead_code)]
mod support;
use pty_runtime::{
    checkpoint::{CheckpointCapacity, CheckpointError, CheckpointRef, ProtectedCheckpoint},
    ports::ICheckpointStore,
    terminal::TerminalConfig,
    *,
};
use pty_runtime_infrastructure::{
    checkpoint::{CheckpointProtector, FileCheckpointStore},
    process::UnixProcessBackend,
    registry::MemorySessionRepository,
    scheduling::{
        BoundedBlockingExecutor, CondvarCapacitySignal, MonotonicSystemClock, StdWorkScheduler,
    },
    terminal::GhosttyTerminalFactory,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use support::*;

/// Serves reads until told to stop. Everything else is the real store, so the
/// park itself is genuine: real encoding, real AEAD, real bytes on disk.
struct BreakableStore {
    inner: FileCheckpointStore,
    broken: AtomicBool,
}
impl ICheckpointStore for BreakableStore {
    fn commit(&self, checkpoint: &ProtectedCheckpoint) -> Result<CheckpointRef, CheckpointError> {
        self.inner.commit(checkpoint)
    }
    fn read(
        &self,
        reference: CheckpointRef,
        max_bytes: usize,
    ) -> Result<ProtectedCheckpoint, CheckpointError> {
        if self.broken.load(Ordering::Acquire) {
            return Err(CheckpointError::Unavailable);
        }
        self.inner.read(reference, max_bytes)
    }
    fn delete(&self, reference: CheckpointRef) -> Result<(), CheckpointError> {
        self.inner.delete(reference)
    }
    fn capacity(&self) -> CheckpointCapacity {
        self.inner.capacity()
    }
}

fn until(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Read until `marker` appears, returning everything seen.
fn read_until(attachment: &mut Attachment, marker: &[u8]) -> Vec<u8> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut seen = Vec::new();
    while !seen.windows(marker.len()).any(|w| w == marker) {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {:?}; saw {:?}",
            String::from_utf8_lossy(marker),
            String::from_utf8_lossy(&seen),
        );
        match block_on(attachment.read_next()).unwrap() {
            OutputEvent::Replay(ReplayPage::Bytes { bytes, .. }) => seen.extend_from_slice(&bytes),
            OutputEvent::Replay(_) => {}
            OutputEvent::Complete(_) => break,
        }
    }
    seen
}

#[test]
fn an_unrestorable_checkpoint_leaves_a_real_child_fully_usable() {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let parent = std::env::temp_dir().join(format!(
        "pty-runtime-unrestorable-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&parent).unwrap();
    let store = Arc::new(BreakableStore {
        inner: FileCheckpointStore::temporary(Some(&parent), 32 * 1024 * 1024).unwrap(),
        broken: AtomicBool::new(false),
    });
    let options = RuntimeOptions::default();
    let backend = Arc::new(
        UnixProcessBackend::new(vec![std::env::current_dir().unwrap()], options.max_sessions)
            .unwrap(),
    );
    let protector = Arc::new(
        CheckpointProtector::new(options.projection.checkpoint_bytes.min(8 * 1024 * 1024)).unwrap(),
    );
    let owner = Runtime::with_projection_adapters(
        options,
        Arc::new(MemorySessionRepository::default()),
        backend,
        ProjectionServices {
            terminal: Arc::new(GhosttyTerminalFactory),
            clock: Arc::new(MonotonicSystemClock::default()),
            scheduler: Arc::new(StdWorkScheduler::with_workers(2, 1).unwrap()),
            blocking: Arc::new(BoundedBlockingExecutor::new(2, 1).unwrap()),
            capacity: Arc::new(CondvarCapacitySignal::default()),
            store: store.clone(),
            protector,
        },
    )
    .unwrap();

    let config = TerminalConfig::new(TerminalSize::new(80, 24).unwrap());
    let mut session_options = SessionOptions::projected(config);
    session_options.projection.as_mut().unwrap().park_after = Duration::from_millis(50);
    let session = owner
        .spawn(
            id("unrestorable-real"),
            &command("echo", &[]),
            session_options,
        )
        .unwrap();
    let pid = session.process_id().unwrap();
    assert!(pid > 0, "a real child was launched");

    let mut attachment = session.attach(AttachPosition::Oldest).unwrap();
    read_until(&mut attachment, b"ready");
    until("the idle session to park", || {
        session.projection_status().unwrap().unwrap().residency == Residency::Parked
    });

    // The source is committed and the live model released. Make it unreadable,
    // then make the child produce output: restoring is now impossible.
    store.broken.store(true, Ordering::Release);
    assert_eq!(
        block_on(session.write(b"after-park\n").unwrap()).written,
        11,
        "the real child accepts input while parked",
    );
    until("projection to report unavailable", || {
        session.projection_status().unwrap().unwrap().residency == Residency::Failed
    });
    assert!(
        session
            .projection_status()
            .unwrap()
            .unwrap()
            .failure
            .is_some(),
        "an unrestorable checkpoint must say why projection is unavailable",
    );

    // Everything below is the ADR's second half, against the real child.
    assert_eq!(
        session.process_id().unwrap(),
        pid,
        "the same Unix child is still owned",
    );
    // The write above was echoed back through the real PTY, and the reader is
    // still delivering it after the projection failed.
    read_until(&mut attachment, b"after-park");

    // Raw input still reaches the child and its echo still comes back, so the
    // PTY writer, the reader and replay all survived.
    assert_eq!(
        block_on(session.write(b"after-failure\n").unwrap()).written,
        14,
        "bounded raw input is still admitted after projection failure",
    );
    read_until(&mut attachment, b"after-failure");

    // Projected observation is correctly refused; raw observation is not.
    assert!(
        session.projected_view().is_err() || block_on(session.projected_view().unwrap()).is_err(),
        "the failed projection does not silently serve a reset model",
    );

    // Cancellation and exit supervision remain usable.
    session.cancel().expect("cancellation stays usable");
    let completion = block_on(session.wait().unwrap()).unwrap();
    assert!(
        completion.status.exit.is_some() || completion.status.supervision_error.is_some(),
        "the child was actually reaped, not abandoned: {:?}",
        completion.status,
    );
    owner.shutdown();
    let _ = std::fs::remove_dir_all(&parent);
}
