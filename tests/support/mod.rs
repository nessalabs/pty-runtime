use std::{
    future::Future,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
    time::{Duration, Instant},
};
struct ThreadWake(std::thread::Thread);
impl Wake for ThreadWake {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}
pub fn block_on<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let waker = Waker::from(Arc::new(ThreadWake(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut cx) {
            return value;
        }
        assert!(Instant::now() < deadline, "fixture deadline exceeded");
        std::thread::park_timeout(deadline.saturating_duration_since(Instant::now()));
    }
}
pub fn command(mode: &str, args: &[&str]) -> pty_runtime::CommandSpec {
    let mut values = vec![mode.into()];
    values.extend(args.iter().map(|v| (*v).into()));
    pty_runtime::CommandSpec::new(
        env!("CARGO_BIN_EXE_pty-runtime-fixture").into(),
        std::env::current_dir().unwrap(),
        values,
    )
    .unwrap()
}
pub fn runtime(options: pty_runtime::RuntimeOptions) -> pty_runtime::Runtime {
    pty_runtime::Runtime::new(vec![std::env::current_dir().unwrap()], options).unwrap()
}
pub fn options() -> pty_runtime::SessionOptions {
    pty_runtime::SessionOptions::raw(pty_runtime::TerminalSize::new(80, 24).unwrap())
}
pub fn id(name: &str) -> pty_runtime::SessionId {
    pty_runtime::SessionId::new(name.into()).unwrap()
}
