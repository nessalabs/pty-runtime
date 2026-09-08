use super::{
    backend::Shared,
    io,
    lifecycle::OwnedProcess,
    session::{Queues, Session, pair},
};
use pty_runtime_domain::process::ProcessError;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, atomic::AtomicBool},
};

pub(super) fn start(
    prepared: super::spawner::Prepared,
    shared: &Shared,
) -> Result<OwnedProcess, ProcessError> {
    let super::spawner::Prepared { request, mut child } = prepared;
    let Some((guardian, host)) = child.take() else {
        return Err(ProcessError::Internal);
    };
    // Guardian's RAII cleanup also covers every registration failure below.
    let reader_host = host.try_clone().map_err(super::error)?;
    let (reader_wake, reader_rx) = pair()?;
    let session = Arc::new(Session {
        diagnostics: request.events.diagnostics(),
        pid: guardian.id(),
        limits: request.limits.clone(),
        queues: Mutex::new(Queues {
            input: VecDeque::new(),
            bytes: 0,
            resize: None,
        }),
        cancel: AtomicBool::new(false),
        cancel_timing: Mutex::new(None),
        closed: AtomicBool::new(false),
        stop_reader: AtomicBool::new(false),
        reader_done: AtomicBool::new(false),
        reader_failed: AtomicBool::new(false),
        wake: Mutex::new(Some(shared.wake.clone())),
        reader_wake: Mutex::new(Some(reader_wake)),
    });
    let reader_session = session.clone();
    let events = request.events.clone();
    let reader = std::thread::Builder::new()
        .name("pty-reader".into())
        .stack_size(request.limits.reader_stack_bytes)
        .spawn(move || io::reader(reader_host, reader_rx, reader_session, events))
        .map_err(super::error)?;
    let mut process = OwnedProcess::new(
        guardian,
        request.admission,
        host,
        session,
        request.events.clone(),
        reader,
    );
    process.reap();
    Ok(process)
}
