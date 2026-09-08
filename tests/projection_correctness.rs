//! Independent bounded regressions for projection/runtime ownership failures.
mod terminal {
    pub use pty_runtime_application::terminal::{ITerminal, ITerminalFactory};
}
mod checkpoint {
    pub use pty_runtime_application::checkpoint::{ICheckpointProtector, ICheckpointStore};
}
#[path = "../crates/application/src/projection/tests/terminal.rs"]
mod native_fixture;
#[path = "../crates/application/src/projection/tests/providers.rs"]
mod provider_fixture;
use pty_runtime::{ports::*, *};
use pty_runtime_infrastructure::{registry::MemorySessionRepository, scheduling::*};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

struct PanicResize;
impl IProcessSession for PanicResize {
    fn process_id(&self) -> u32 {
        42
    }
    fn write_reserved(
        &self,
        bytes: &[u8],
        _: Option<Box<dyn IInputReservation>>,
    ) -> Result<ProcessOperation<WriteOutcome>, ProcessError> {
        let written = bytes.len();
        Ok(Box::pin(async move {
            WriteOutcome {
                written,
                error: None,
            }
        }))
    }
    fn request_cancel(&self) -> Result<(), ProcessError> {
        Ok(())
    }
    fn resize(
        &self,
        _: TerminalSize,
    ) -> Result<ProcessOperation<Result<(), ProcessError>>, ProcessError> {
        Ok(Box::pin(async {
            panic!("injected admitted process-future failure")
        }))
    }
}
struct Backend;
impl IProcessBackend for Backend {
    fn spawn(
        &self,
        _: &CommandSpec,
        _: TerminalSize,
        _: SessionLifetime,
        _: ProcessLimits,
        _: Arc<dyn IProcessEvents>,
    ) -> Result<Arc<dyn IProcessSession>, ProcessError> {
        Ok(Arc::new(PanicResize))
    }
    fn shutdown(&self) {}
    fn shutdown_now(&self) {}
}
fn owner() -> Runtime {
    configured_owner(RuntimeOptions::default(), Arc::new(Backend))
}
fn configured_owner(options: RuntimeOptions, backend: Arc<dyn IProcessBackend>) -> Runtime {
    Runtime::with_projection_adapters(
        options,
        Arc::new(MemorySessionRepository::default()),
        backend,
        ProjectionServices {
            terminal: Arc::new(native_fixture::Factory(Arc::new(
                native_fixture::Probe::default(),
            ))),
            clock: Arc::new(MonotonicSystemClock::default()),
            scheduler: Arc::new(StdWorkScheduler::with_workers(2, 1).unwrap()),
            blocking: Arc::new(BoundedBlockingExecutor::new(2, 1).unwrap()),
            capacity: Arc::new(CondvarCapacitySignal::default()),
            store: Arc::new(provider_fixture::Store::default()),
            protector: Arc::new(provider_fixture::Protector::default()),
        },
    )
    .unwrap()
}
#[test]
fn process_future_panic_does_not_strand_runtime_shutdown() {
    let runtime = owner();
    let config = terminal_config();
    let session = runtime
        .spawn(
            SessionId::new("panic-resize".into()).unwrap(),
            &CommandSpec::new("/unused".into(), "/".into(), vec![]).unwrap(),
            SessionOptions::projected(config),
        )
        .unwrap();
    let _resize = session
        .resize_projected(TerminalSize::new(3, 1).unwrap())
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while session
        .projection_status()
        .unwrap()
        .unwrap()
        .failure
        .is_none()
    {
        assert!(Instant::now() < deadline, "injected future was not polled");
        std::thread::sleep(Duration::from_millis(1));
    }
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        runtime.shutdown();
        send.send(()).unwrap();
    });
    assert!(
        receive.recv_timeout(Duration::from_secs(2)).is_ok(),
        "scheduler panic containment must retain a cleanup path; shutdown cannot wait forever for a retired registration"
    );
    assert_eq!(
        session.projection_status().unwrap().unwrap().residency,
        Residency::Closed
    );
}
fn terminal_config() -> pty_runtime::terminal::TerminalConfig {
    pty_runtime::terminal::TerminalConfig {
        size: TerminalSize::new(2, 1).unwrap(),
        history_bytes: 128,
        continuation_bytes: 128,
        reply_bytes: 32,
        checkpoint_bytes: 1024,
        native_bytes: 4096,
        view_bytes: 1024,
        feed_bytes: 8192,
    }
}

#[derive(Default)]
struct CountBackend(std::sync::atomic::AtomicUsize);
impl IProcessBackend for CountBackend {
    fn spawn(
        &self,
        _: &CommandSpec,
        _: TerminalSize,
        _: SessionLifetime,
        _: ProcessLimits,
        _: Arc<dyn IProcessEvents>,
    ) -> Result<Arc<dyn IProcessSession>, ProcessError> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(Arc::new(PanicResize))
    }
    fn shutdown(&self) {}
    fn shutdown_now(&self) {}
}
#[test]
fn impossible_global_parser_budgets_reject_before_child_launch() {
    for constrain_reply in [false, true] {
        let mut options = RuntimeOptions::default();
        if constrain_reply {
            options.projection.view_bytes = 1;
        } else {
            options.projection.staging_bytes = 1;
        }
        let backend = Arc::new(CountBackend::default());
        let runtime = configured_owner(options, backend.clone());
        let result = runtime.spawn(
            SessionId::new("impossible-capacity".into()).unwrap(),
            &CommandSpec::new("/unused".into(), "/".into(), vec![]).unwrap(),
            SessionOptions::projected(terminal_config()),
        );
        let rejected = result.is_err();
        runtime.shutdown();
        assert!(
            rejected,
            "a parser chunk/reply reservation larger than its entire global pool can never make progress"
        );
        assert_eq!(backend.0.load(std::sync::atomic::Ordering::SeqCst), 0);
    }
}

#[test]
fn shutdown_closes_model_with_completed_request_wait_retained() {
    let mut options = RuntimeOptions::default();
    options.projection.request_slots = 1;
    let runtime = configured_owner(options, Arc::new(Backend));
    let mut session_options = SessionOptions::projected(terminal_config());
    session_options.projection.as_mut().unwrap().request_slots = 1;
    let session = runtime
        .spawn(
            SessionId::new("retained-request".into()).unwrap(),
            &CommandSpec::new("/unused".into(), "/".into(), vec![]).unwrap(),
            session_options,
        )
        .unwrap();
    let mut held = session.projected_view().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    let copied_view = loop {
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        if let std::task::Poll::Ready(value) = held.as_mut().poll(&mut context) {
            break value.unwrap();
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    };
    assert!(
        session.projected_view().is_err(),
        "the retained wait holds the sole request slot"
    );
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        runtime.shutdown();
        send.send(()).unwrap();
    });
    assert!(receive.recv_timeout(Duration::from_secs(2)).is_ok());
    assert_eq!(
        session.projection_status().unwrap().unwrap().residency,
        Residency::Closed
    );
    assert_eq!(copied_view.view().size, terminal_config().size);
    drop(held);
}

struct LatePanicBackend(Arc<std::sync::atomic::AtomicBool>);
impl IProcessBackend for LatePanicBackend {
    fn spawn(
        &self,
        _: &CommandSpec,
        _: TerminalSize,
        _: SessionLifetime,
        _: ProcessLimits,
        _: Arc<dyn IProcessEvents>,
    ) -> Result<Arc<dyn IProcessSession>, ProcessError> {
        Ok(Arc::new(LatePanicProcess(self.0.clone())))
    }
    fn shutdown(&self) {}
    fn shutdown_now(&self) {}
}
struct LatePanicProcess(Arc<std::sync::atomic::AtomicBool>);
impl IProcessSession for LatePanicProcess {
    fn process_id(&self) -> u32 {
        43
    }
    fn write_reserved(
        &self,
        _: &[u8],
        _: Option<Box<dyn IInputReservation>>,
    ) -> Result<ProcessOperation<WriteOutcome>, ProcessError> {
        Err(ProcessError::Io)
    }
    fn request_cancel(&self) -> Result<(), ProcessError> {
        Ok(())
    }
    fn resize(
        &self,
        _: TerminalSize,
    ) -> Result<ProcessOperation<Result<(), ProcessError>>, ProcessError> {
        let polled = self.0.clone();
        Ok(Box::pin(std::future::poll_fn(move |_| {
            if polled.swap(true, std::sync::atomic::Ordering::AcqRel) {
                panic!("injected panic while polling closing resize");
            }
            std::task::Poll::Pending
        })))
    }
}
#[test]
fn resize_panic_during_close_keeps_cleanup_scheduled() {
    let polled = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let runtime = configured_owner(
        RuntimeOptions::default(),
        Arc::new(LatePanicBackend(polled.clone())),
    );
    let session = runtime
        .spawn(
            SessionId::new("closing-panic".into()).unwrap(),
            &CommandSpec::new("/unused".into(), "/".into(), vec![]).unwrap(),
            SessionOptions::projected(terminal_config()),
        )
        .unwrap();
    let _resize = session
        .resize_projected(TerminalSize::new(3, 1).unwrap())
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !polled.load(std::sync::atomic::Ordering::Acquire) {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        runtime.shutdown();
        send.send(()).unwrap();
    });
    assert!(
        receive.recv_timeout(Duration::from_secs(2)).is_ok(),
        "a panic consuming the close wake must schedule remaining cleanup"
    );
    assert_eq!(
        session.projection_status().unwrap().unwrap().residency,
        Residency::Closed
    );
}
