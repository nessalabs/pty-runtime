//! A public clock failure before output admission must not publish partial work.
use super::*;
use std::{
    future::Future,
    pin::Pin,
    sync::{Mutex, atomic::Ordering},
    task::{Context, Poll, Waker},
};

#[derive(Default)]
struct CallerClock(Mutex<Option<std::thread::ThreadId>>);
impl IClock for CallerClock {
    fn now(&self) -> Duration {
        let should_panic = {
            let mut caller = self.0.lock().unwrap();
            if caller.as_ref() == Some(&std::thread::current().id()) {
                caller.take();
                true
            } else {
                false
            }
        };
        assert!(!should_panic, "injected output admission clock failure");
        Duration::ZERO
    }
}

#[derive(Default)]
struct EventBackend(Mutex<Option<Arc<dyn IProcessEvents>>>);
impl IProcessBackend for EventBackend {
    fn spawn(
        &self,
        command: &CommandSpec,
        size: TerminalSize,
        lifetime: SessionLifetime,
        limits: ProcessLimits,
        events: Arc<dyn IProcessEvents>,
    ) -> Result<Arc<dyn IProcessSession>, ProcessError> {
        *self.0.lock().unwrap() = Some(events.clone());
        Backend.spawn(command, size, lifetime, limits, events)
    }
    fn shutdown(&self) {}
    fn shutdown_now(&self) {}
}

#[test]
fn clock_panic_rejects_later_output_without_publication_and_owner_still_cleans_up() {
    let clock = Arc::new(CallerClock::default());
    let probe = Arc::new(native_fixture::Probe::default());
    let backend = Arc::new(EventBackend::default());
    let diagnostics = RuntimeDiagnostics::new();
    let mut services = projection_services(probe.clone());
    services.clock = clock.clone();
    let runtime = Runtime::with_projection_adapters(
        RuntimeOptions::default(),
        Arc::new(MemorySessionRepository::default()),
        backend.clone(),
        services,
    )
    .unwrap()
    .with_diagnostics(diagnostics.clone());
    let session = runtime
        .spawn(
            SessionId::new("clock-failure".into()).unwrap(),
            &CommandSpec::new("/unused".into(), "/".into(), vec![]).unwrap(),
            SessionOptions::projected(terminal_config()),
        )
        .unwrap();
    let events = backend.0.lock().unwrap().clone().unwrap();
    assert_eq!(diagnostics.aggregate().active_sessions, 1);
    assert_eq!(probe.alive.load(Ordering::Acquire), 1);
    // Worker clock reads cannot consume this fault: only this callback thread can.
    *clock.0.lock().unwrap() = Some(std::thread::current().id());
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            events.output(b"unpublished");
        }))
        .is_err()
    );
    assert!(
        clock.0.lock().unwrap().is_none(),
        "clock fault was not consumed"
    );
    assert_eq!(session.status(), Err(RuntimeError::Internal));
    assert!(matches!(session.wait(), Err(RuntimeError::Internal)));
    assert_eq!(runtime.resources().observers.used, 0);
    assert_eq!(events.output(b"later"), OutputAcceptance::Closed);
    let status = session.projection_status().unwrap().unwrap();
    assert_eq!(status.published.offset, 0);
    assert_eq!(status.processed.offset, 0);
    assert_eq!(runtime.resources().replay_capacity.used, 0);
    let projection = runtime.resources().projection.unwrap();
    assert_eq!(projection.staging_bytes.used, 0);
    assert_eq!(projection.staging_slots.used, 0);

    runtime.shutdown();
    assert_eq!(
        session.projection_status().unwrap().unwrap().residency,
        Residency::Closed
    );
    assert_eq!(probe.alive.load(Ordering::Acquire), 0);
    assert!(
        !probe
            .trace
            .lock()
            .unwrap()
            .iter()
            .any(|entry| matches!(entry, native_fixture::Trace::Feed(_)))
    );
    assert_eq!(
        runtime
            .resources()
            .projection
            .unwrap()
            .native_reservations
            .used,
        0
    );
    drop(session);
    drop(runtime);
    assert_eq!(diagnostics.aggregate().active_sessions, 0);
    assert_eq!(diagnostics.aggregate().retained_replay_bytes, 0);
}

#[test]
fn existing_output_and_completion_waits_report_internal_error_after_clock_failure() {
    let clock = Arc::new(CallerClock::default());
    let backend = Arc::new(EventBackend::default());
    let mut services = projection_services(Arc::new(native_fixture::Probe::default()));
    services.clock = clock.clone();
    let runtime = Runtime::with_projection_adapters(
        RuntimeOptions::default(),
        Arc::new(MemorySessionRepository::default()),
        backend.clone(),
        services,
    )
    .unwrap();
    let session = runtime
        .spawn(
            SessionId::new("clock-waits".into()).unwrap(),
            &CommandSpec::new("/unused".into(), "/".into(), vec![]).unwrap(),
            SessionOptions::projected(terminal_config()),
        )
        .unwrap();
    let mut completion = session.wait().unwrap();
    let mut attachment = session.attach(AttachPosition::Oldest).unwrap();
    let cursor = attachment.cursor();
    let mut output = attachment.read_next();
    let mut context = Context::from_waker(Waker::noop());
    assert!(Pin::new(&mut completion).poll(&mut context).is_pending());
    assert!(Pin::new(&mut output).poll(&mut context).is_pending());

    let events = backend.0.lock().unwrap().clone().unwrap();
    *clock.0.lock().unwrap() = Some(std::thread::current().id());
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            events.output(b"unpublished");
        }))
        .is_err()
    );
    assert!(clock.0.lock().unwrap().is_none());
    assert!(matches!(
        Pin::new(&mut completion).poll(&mut context),
        Poll::Ready(Err(RuntimeError::Internal))
    ));
    assert!(matches!(
        Pin::new(&mut output).poll(&mut context),
        Poll::Ready(Err(RuntimeError::Internal))
    ));
    drop(output);
    assert_eq!(attachment.cursor(), cursor);
    assert!(matches!(attachment.try_next(), Err(RuntimeError::Internal)));
    drop((completion, attachment));
    runtime.shutdown();
    assert_eq!(
        session.projection_status().unwrap().unwrap().residency,
        Residency::Closed
    );
}
