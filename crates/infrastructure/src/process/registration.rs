use super::{
    backend::Shared,
    io,
    lifecycle::OwnedProcess,
    session::{Queues, Session, pair},
    signals::signal,
    watch::ExitWatch,
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
    let Some((mut child, host)) = child.take() else {
        return Err(ProcessError::Internal);
    };
    let mut already_reaped = false;
    let prepared = (|| {
        let reader_host = host.try_clone().map_err(super::error)?;
        let (reader_wake, reader_rx) = pair()?;
        let session = Arc::new(Session {
            pid: child.id(),
            limits: request.limits.clone(),
            queues: Mutex::new(Queues {
                input: VecDeque::new(),
                bytes: 0,
                resize: None,
            }),
            cancel: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            stop_reader: AtomicBool::new(false),
            reader_done: AtomicBool::new(false),
            reader_failed: AtomicBool::new(false),
            wake: shared.wake.clone(),
            reader_wake,
        });
        let watcher = ExitWatch::new(child.id());
        // A child may exit before registration. Only actual wait status makes this safe.
        let initial_exit = if watcher.is_err() {
            child.try_wait().map_err(super::error)?
        } else {
            None
        };
        already_reaped = initial_exit.is_some();
        if watcher.is_err() && initial_exit.is_none() {
            return Err(ProcessError::Io);
        }
        let reader_session = session.clone();
        let events = request.events.clone();
        let reader = std::thread::Builder::new()
            .name("pty-reader".into())
            .stack_size(request.limits.reader_stack_bytes)
            .spawn(move || io::reader(reader_host, reader_rx, reader_session, events))
            .map_err(super::error)?;
        Ok((session, watcher.ok(), reader, initial_exit))
    })();
    match prepared {
        Ok((session, watch, reader, initial_exit)) => {
            let mut process = OwnedProcess {
                child,
                _admission: request.admission,
                host,
                session,
                watch,
                reader: Some(reader),
                events: request.events.clone(),
                exit_at: None,
                cancel_at: None,
                killed: false,
                supervision_lost: false,
            };
            if let Some(status) = initial_exit {
                process.record_exit(status);
            }
            Ok(process)
        }
        Err(error) => {
            // The supervisor exclusively owns this unreaped child; no competing waiter.
            if !already_reaped {
                signal(&child, &host, libc::SIGKILL);
                let _ = child.wait();
            }
            Err(error)
        }
    }
}
