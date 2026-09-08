use super::{LatencyKind, RuntimeDiagnostics, Timing};
use crate::process::{IInputReservation, IProcessSession, ProcessOperation};
use pty_runtime_domain::{
    process::{ProcessError, WriteOutcome},
    terminal::TerminalSize,
};
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    task::{Context, Poll, Waker},
    time::Instant,
};

struct UntimedAdapter(AtomicUsize);
impl IProcessSession for UntimedAdapter {
    fn process_id(&self) -> u32 {
        0
    }
    fn write_reserved(
        &self,
        _: &[u8],
        _: Option<Box<dyn IInputReservation>>,
    ) -> Result<ProcessOperation<WriteOutcome>, ProcessError> {
        Err(ProcessError::Unsupported)
    }
    fn request_cancel(&self) -> Result<(), ProcessError> {
        Err(ProcessError::Unsupported)
    }
    fn resize(
        &self,
        _: TerminalSize,
    ) -> Result<ProcessOperation<Result<(), ProcessError>>, ProcessError> {
        self.0.fetch_add(1, Ordering::Relaxed);
        Ok(Box::pin(async { Ok(()) }))
    }
}

#[test]
fn untimed_replacement_adapter_reports_unavailable_without_false_success_or_failure() {
    let adapter = UntimedAdapter(AtomicUsize::new(0));
    let diagnostics = RuntimeDiagnostics::new();
    let timing = Timing::new(
        diagnostics.clone(),
        LatencyKind::ResizeDispatch,
        Instant::now(),
    );
    let mut operation = adapter
        .resize_timed(TerminalSize::new(80, 24).unwrap(), Some(timing))
        .unwrap();
    assert_eq!(
        operation
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop())),
        Poll::Ready(Ok(()))
    );
    assert_eq!(adapter.0.load(Ordering::Relaxed), 1);
    let measurement = diagnostics.snapshot(LatencyKind::ResizeDispatch);
    assert_eq!(measurement.unavailable, 1);
    assert_eq!(measurement.samples(), 0);
    assert_eq!(measurement.failures, 0);
    assert_eq!(measurement.maximum_us, 0);
    // This is materially different from an instrumented operation abandoned
    // before its boundary: that is a failure, and must never enter percentiles.
    drop(Timing::new(
        diagnostics.clone(),
        LatencyKind::ResizeDispatch,
        Instant::now(),
    ));
    let abandoned = diagnostics.snapshot(LatencyKind::ResizeDispatch);
    assert_eq!(abandoned.unavailable, 1);
    assert_eq!(abandoned.failures, 1);
    assert_eq!(abandoned.samples(), 0);
}
