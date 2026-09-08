//! One live, TERM-ignoring dispatch probe; its grace wait never stalls observers.
use super::{DEADLINE, Result, population::Child};
use pty_runtime::{CompletionWait, DrainOutcome, ExitStatus, Runtime};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
    time::Instant,
};
pub struct Probe {
    child: Child,
    wait: CompletionWait,
    started: Instant,
}
impl Probe {
    pub fn start(child: Child) -> Result<Self> {
        child.session.cancel()?;
        let wait = child.session.wait()?;
        Ok(Self {
            child,
            wait,
            started: Instant::now(),
        })
    }
    pub fn poll(&mut self, runtime: &Runtime) -> Result<bool> {
        let mut context = Context::from_waker(Waker::noop());
        if let Poll::Ready(result) = Pin::new(&mut self.wait).poll(&mut context) {
            let completion = result?;
            assert!(
                completion.status.supervision_error.is_none(),
                "{completion:?}"
            );
            assert_eq!(completion.status.exit, Some(ExitStatus::Signal(9)));
            assert_eq!(completion.status.drain, Some(DrainOutcome::Eof));
            runtime.forget(&self.child.id)?;
            return Ok(true);
        }
        assert!(self.started.elapsed() < DEADLINE, "cancel probe timed out");
        Ok(false)
    }
}
