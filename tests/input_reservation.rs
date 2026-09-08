//! A dropped input waiter cannot release memory still owned by an adapter queue.
use pty_runtime::{ports::*, *};
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct HeldInput {
    reservations: Mutex<Vec<Option<Box<dyn IInputReservation>>>>,
}
impl IProcessSession for HeldInput {
    fn process_id(&self) -> u32 {
        1
    }
    fn write_reserved(
        &self,
        _bytes: &[u8],
        reservation: Option<Box<dyn IInputReservation>>,
    ) -> Result<ProcessOperation<WriteOutcome>, ProcessError> {
        self.reservations.lock().unwrap().push(reservation);
        Ok(Box::pin(std::future::pending()))
    }
    fn request_cancel(&self) -> Result<(), ProcessError> {
        Ok(())
    }
    fn resize(
        &self,
        _size: TerminalSize,
    ) -> Result<ProcessOperation<Result<(), ProcessError>>, ProcessError> {
        Ok(Box::pin(async { Ok(()) }))
    }
}
struct Backend(Arc<HeldInput>);
impl IProcessBackend for Backend {
    fn spawn(
        &self,
        _command: &CommandSpec,
        _size: TerminalSize,
        _lifetime: SessionLifetime,
        _limits: ProcessLimits,
        _events: Arc<dyn IProcessEvents>,
    ) -> Result<Arc<dyn IProcessSession>, ProcessError> {
        Ok(self.0.clone())
    }
    fn shutdown(&self) {
        self.0.reservations.lock().unwrap().clear();
    }
    fn shutdown_now(&self) {
        self.shutdown();
    }
}
#[derive(Default)]
struct Registry(Mutex<std::collections::HashMap<SessionId, Arc<SessionContext>>>);
impl ISessionRepository for Registry {
    fn register(
        &self,
        id: SessionId,
        context: Arc<SessionContext>,
        capacity: usize,
    ) -> Result<(), RuntimeError> {
        let mut map = self.0.lock().unwrap();
        if map.contains_key(&id) {
            return Err(RuntimeError::ExistingSession);
        }
        if map.len() >= capacity {
            return Err(RuntimeError::Capacity);
        }
        map.insert(id, context);
        Ok(())
    }
    fn lookup(&self, id: &SessionId) -> Result<Arc<SessionContext>, RuntimeError> {
        self.0
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or(RuntimeError::MissingSession)
    }
    fn remove_finished(
        &self,
        _id: &SessionId,
        _lifetime: SessionLifetime,
    ) -> Result<(), RuntimeError> {
        Err(RuntimeError::NotFinished)
    }
    fn rollback_spawn(&self, id: &SessionId, _lifetime: SessionLifetime) {
        self.0.lock().unwrap().remove(id);
    }
}
#[test]
fn global_bytes_and_slots_survive_abandoned_wait_and_release_on_adapter_completion() {
    let held = Arc::new(HeldInput::default());
    let runtime = Runtime::with_adapters(
        RuntimeOptions {
            input_bytes: 4,
            input_slots: 1,
            ..RuntimeOptions::default()
        },
        Arc::new(Registry::default()),
        Arc::new(Backend(held.clone())),
    )
    .unwrap();
    let options = SessionOptions::raw(TerminalSize::new(80, 24).unwrap());
    let command = CommandSpec::new("/bin/test".into(), "/tmp".into(), vec![]).unwrap();
    let first = runtime
        .spawn(
            SessionId::new("first".into()).unwrap(),
            &command,
            options.clone(),
        )
        .unwrap();
    let second = runtime
        .spawn(SessionId::new("second".into()).unwrap(), &command, options)
        .unwrap();
    let wait = first.write(b"1234").unwrap();
    drop(wait);
    assert!(matches!(second.write(b"x"), Err(RuntimeError::Capacity)));
    held.reservations.lock().unwrap().clear();
    assert!(matches!(
        second.write(b"12345"),
        Err(RuntimeError::Capacity)
    ));
    let wait: Pin<Box<dyn Future<Output = WriteOutcome> + Send>> = second.write(b"abcd").unwrap();
    drop(wait);
    assert!(matches!(first.write(b"x"), Err(RuntimeError::Capacity)));
    held.reservations.lock().unwrap().clear();
    assert!(first.write(b"x").is_ok());
}
