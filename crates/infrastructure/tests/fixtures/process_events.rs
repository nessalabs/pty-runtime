//! Shared real-process fixture collection and bounded waits.
#![allow(dead_code, unused_imports)]
use pty_runtime_application::process::{IProcessBackend, IProcessEvents, OutputAcceptance};
use pty_runtime_domain::{
    SessionLifetime,
    process::{
        CommandSpec, DrainOutcome, EnvironmentPolicy, ExitStatus, ProcessError, ProcessLimits,
    },
    terminal::TerminalSize,
};
use std::{
    future::Future,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    task::{Context, Poll, Wake, Waker},
    time::{Duration, Instant},
};
#[derive(Default)]
pub struct State {
    pub bytes: Vec<u8>,
    pub exit: Option<ExitStatus>,
    pub drain: Option<DrainOutcome>,
    pub failure: Option<ProcessError>,
}
#[derive(Default)]
pub struct Events {
    pub state: Mutex<State>,
    pub changed: Condvar,
    pub blocked: bool,
}
impl IProcessEvents for Events {
    fn output(&self, bytes: &[u8]) -> OutputAcceptance {
        if self.blocked {
            return OutputAcceptance::Backpressure;
        }
        self.state.lock().unwrap().bytes.extend_from_slice(bytes);
        self.changed.notify_all();
        OutputAcceptance::Accepted
    }
    fn wait_for_capacity(&self, deadline: Instant) {
        let state = self.state.lock().unwrap();
        drop(
            self.changed
                .wait_timeout(state, deadline.saturating_duration_since(Instant::now()))
                .unwrap(),
        );
    }
    fn exited(&self, status: ExitStatus) {
        self.state.lock().unwrap().exit = Some(status);
        self.changed.notify_all();
    }
    fn drained(&self, outcome: DrainOutcome) {
        self.state.lock().unwrap().drain = Some(outcome);
        self.changed.notify_all();
    }
    fn supervision_failed(&self, error: ProcessError) {
        self.state.lock().unwrap().failure = Some(error);
        self.changed.notify_all();
    }
}
impl Events {
    pub fn wait(&self, predicate: impl Fn(&State) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut state = self.state.lock().unwrap();
        while !predicate(&state) {
            assert!(
                Instant::now() < deadline,
                "timed out; bytes={:?}, exit={:?}, drain={:?}",
                state.bytes,
                state.exit,
                state.drain
            );
            state = self
                .changed
                .wait_timeout(state, deadline.saturating_duration_since(Instant::now()))
                .unwrap()
                .0;
        }
        assert_eq!(state.failure, None);
    }
}
struct ThreadWake(std::thread::Thread);
impl Wake for ThreadWake {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}
pub fn wait<F: Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    let waker = Waker::from(Arc::new(ThreadWake(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(result) => return result,
            Poll::Pending => {
                assert!(Instant::now() < deadline, "operation timed out");
                std::thread::park_timeout(Duration::from_millis(20));
            }
        }
    }
}
pub fn command(script: &str) -> CommandSpec {
    CommandSpec::new(
        PathBuf::from("/bin/sh"),
        std::env::temp_dir(),
        vec!["-c".into(), script.into()],
    )
    .unwrap()
}
pub fn size() -> TerminalSize {
    TerminalSize::new(80, 24).unwrap()
}
