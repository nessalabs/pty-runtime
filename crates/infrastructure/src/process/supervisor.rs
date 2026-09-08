use super::{backend::Shared, io, lifecycle::OwnedProcess};
use pty_runtime_domain::process::ProcessError;
use std::{
    io::Read,
    os::{fd::AsRawFd, unix::net::UnixStream},
    sync::{Arc, atomic::Ordering, mpsc::Receiver},
    time::{Duration, Instant},
};
pub(super) fn run(
    shared: Arc<Shared>,
    mut wake: UnixStream,
    requests: Receiver<super::spawner::Prepared>,
) {
    let mut processes = Vec::<OwnedProcess>::new();
    loop {
        let mut admitted = 0;
        for _ in 0..8 {
            let Ok(request) = requests.try_recv() else {
                break;
            };
            admitted += 1;
            let reply = request.request.reply.clone();
            if shared.shutdown.load(Ordering::Acquire) {
                drop(request);
                let _ = reply.send(Err(ProcessError::Closed));
                continue;
            }
            match super::registration::start(request, &shared) {
                Ok(process) => {
                    let _ = reply.send(Ok(process.session.clone()));
                    processes.push(process);
                }
                Err(error) => {
                    let _ = reply.send(Err(error));
                }
            }
        }
        let shutting = shared.shutdown.load(Ordering::Acquire);
        let now = Instant::now();
        for process in &mut processes {
            process.control(now, shutting, shared.immediate.load(Ordering::Acquire));
        }
        processes.retain_mut(|process| {
            if process.guardian.complete() && process.session.reader_done.load(Ordering::Acquire) {
                if let Some(reader) = process.reader.take() {
                    let _ = reader.join();
                }
                false
            } else {
                true
            }
        });
        if shutting && processes.is_empty() {
            break;
        }
        let mut fds = vec![libc::pollfd {
            fd: wake.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        }];
        let mut timeout = (admitted == 8).then_some(Duration::ZERO);
        for process in &processes {
            let queued = process
                .session
                .queues
                .lock()
                .is_ok_and(|q| !q.input.is_empty());
            fds.extend(process.guardian.pollfds());
            fds.push(libc::pollfd {
                fd: if queued { process.host.as_raw_fd() } else { -1 },
                events: libc::POLLOUT,
                revents: 0,
            });
            if let Some(deadline) = process.deadline() {
                let remaining = deadline.saturating_duration_since(Instant::now());
                timeout = Some(timeout.map_or(remaining, |old| old.min(remaining)));
            }
        }
        let millis = timeout.map_or(-1, |d| {
            d.as_millis().saturating_add(1).min(i32::MAX as u128) as i32
        });
        // SAFETY: fds is initialized and every descriptor remains owned during poll.
        let ready = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as _, millis) };
        if ready < 0 && std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
            shared.shutdown.store(true, Ordering::Release);
        }
        let mut scratch = [0; 256];
        while wake.read(&mut scratch).is_ok_and(|count| count > 0) {}
        for (index, process) in processes.iter_mut().enumerate() {
            if fds[1 + index * 4..4 + index * 4]
                .iter()
                .any(|fd| fd.revents != 0)
                || process.guardian.needs_poll()
            {
                process.reap();
            }
            if process.exit_at.is_none() {
                io::write_ready(&mut process.host, &process.session);
            }
        }
    }
}
