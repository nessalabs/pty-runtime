use super::{error, session::Session};
use pty_runtime_application::process::{IProcessEvents, OutputAcceptance};
use pty_runtime_domain::process::{DrainOutcome, ProcessError, WriteOutcome};
use std::{
    fs::File,
    io::{Read, Write},
    os::{fd::AsRawFd, unix::net::UnixStream},
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};
pub(super) fn reader(
    mut host: File,
    wake: UnixStream,
    session: Arc<Session>,
    events: Arc<dyn IProcessEvents>,
) {
    let mut scratch = vec![0; session.limits.read_chunk];
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        read_loop(&mut host, &wake, &session, &*events, &mut scratch)
    }))
    .unwrap_or(DrainOutcome::Failed(ProcessError::Internal));
    if matches!(outcome, DrainOutcome::Failed(_)) {
        session.reader_failed.store(true, Ordering::Release);
        session.notify();
    }
    scratch.fill(0);
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| events.drained(outcome)));
    session.reader_done.store(true, Ordering::Release);
    session.notify();
}
fn read_loop(
    host: &mut File,
    wake: &UnixStream,
    session: &Session,
    events: &dyn IProcessEvents,
    scratch: &mut [u8],
) -> DrainOutcome {
    loop {
        if session.stop_reader.load(Ordering::Acquire) {
            return DrainOutcome::Truncated;
        }
        let mut fds = [
            libc::pollfd {
                fd: host.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: wake.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        // SAFETY: the array and both owned descriptors stay live across poll.
        if unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as _, -1) } < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return DrainOutcome::Failed(ProcessError::Io);
        }
        if session.stop_reader.load(Ordering::Acquire) {
            return DrainOutcome::Truncated;
        }
        let count = match host.read(scratch) {
            Ok(0) => return DrainOutcome::Eof,
            Ok(count) => count,
            Err(e)
                if e.kind() == std::io::ErrorKind::Interrupted
                    || e.kind() == std::io::ErrorKind::WouldBlock =>
            {
                continue;
            }
            // Linux reports EIO when all child endpoint references close.
            Err(e) if e.raw_os_error() == Some(libc::EIO) => return DrainOutcome::Eof,
            Err(e) => return DrainOutcome::Failed(error(e)),
        };
        let read_completed = Instant::now();
        if let Some(diagnostics) = &session.diagnostics {
            diagnostics.count(
                pty_runtime_application::diagnostics::CounterKind::BytesRead,
                count as u64,
            );
        }
        loop {
            if session.stop_reader.load(Ordering::Acquire) {
                return DrainOutcome::Truncated;
            }
            match events.output_observed(&scratch[..count], Some(read_completed)) {
                OutputAcceptance::Accepted => break,
                OutputAcceptance::Closed => return DrainOutcome::Truncated,
                OutputAcceptance::Backpressure => {
                    if let Some(diagnostics) = &session.diagnostics {
                        diagnostics.count(
                            pty_runtime_application::diagnostics::CounterKind::OutputBackpressure,
                            1,
                        );
                    }
                    events.wait_for_capacity(Instant::now() + Duration::from_millis(20))
                }
            }
        }
    }
}
pub(super) fn write_ready(host: &mut File, session: &Session) {
    let Ok(mut queues) = session.queues.lock() else {
        return;
    };
    let Some(input) = queues.input.front_mut() else {
        return;
    };
    let mut failure = None;
    if Instant::now() >= input.deadline {
        failure = Some(ProcessError::Timeout);
    } else if input.offset < input.bytes.len() {
        match host.write(&input.bytes[input.offset..]) {
            Ok(0) => failure = Some(ProcessError::Io),
            Ok(count) => {
                input.offset += count;
                if let Some(diagnostics) = &session.diagnostics {
                    diagnostics.count(
                        pty_runtime_application::diagnostics::CounterKind::BytesWritten,
                        count as u64,
                    );
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) => {}
            Err(e) => failure = Some(error(e)),
        }
    }
    if failure.is_some() || input.offset == input.bytes.len() {
        if let Some(mut input) = queues.input.pop_front() {
            queues.bytes -= input.bytes.len();
            drop(queues);
            if failure.is_some() {
                if let Some(diagnostics) = &session.diagnostics {
                    diagnostics.count(
                        pty_runtime_application::diagnostics::CounterKind::FailedOperations,
                        1,
                    );
                }
            }
            if let Some(timing) = input.timing.take() {
                timing.finish(failure.is_none());
            }
            if let Some(reply) = input.reply.take() {
                let written = input.offset;
                drop(input);
                let _ = reply.send(WriteOutcome {
                    written,
                    error: failure,
                });
            }
        }
    }
}
