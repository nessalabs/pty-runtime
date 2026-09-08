//! The actual workload's parent owns wait status and normal cancellation.
use crate::{
    Config,
    cancellation::Cancellation,
    cleanup::{Progress, Sweep},
    links::Links,
    os,
    protocol::Kind,
    workload::Workload,
};
use std::{
    io,
    os::fd::{AsRawFd, OwnedFd},
    time::Instant,
};

pub fn run(mut links: Links, host: OwnedFd, slave: OwnedFd, config: Config) -> io::Result<()> {
    os::own_group()?;
    // SAFETY: these read-only calls obtain this helper's stable own identities.
    let pid = unsafe { libc::getpid() };
    links.notify(Kind::Hello, [config.sid, pid, 0, 0]);
    if !await_sentinel(&mut links, config.sid, pid) {
        links.notify(Kind::StartFailed, [libc::ECANCELED, 0, 0, 0]);
        links.flush();
        return Ok(());
    }
    let mut workload = match Workload::launch(&config.arguments, slave) {
        Ok(workload) => workload,
        Err(error) => {
            links.notify(
                Kind::StartFailed,
                [error.raw_os_error().unwrap_or(libc::EIO), 0, 0, 0],
            );
            links.flush();
            return Ok(());
        }
    };
    links.notify(Kind::Started, [workload.pid(), pid, config.sid, config.sid]);
    let mut cancellation = Cancellation::new(
        config.sid,
        pid,
        host.as_raw_fd(),
        config.generation,
        config.grace,
    );
    let mut sweep: Option<Sweep> = None;
    let mut peer_gone = false;
    let mut finish_peer = false;
    let mut cleaning = false;
    let mut drain = crate::drain::Drain::new(config.sid, pid);
    loop {
        let now = Instant::now();
        let (owner_messages, peer_messages) = links.inbox();
        for frame in owner_messages.into_iter().chain(peer_messages).flatten() {
            match frame.kind {
                Kind::Terminate | Kind::Kill => cancellation.request(frame.kind),
                Kind::Release | Kind::Abort => cleaning = true,
                Kind::Hello | Kind::Execute => {}
                Kind::Fault => {
                    links.fault();
                    cleaning = true;
                }
                Kind::Retiring if finish_peer => {}
                _ => {
                    links.fault();
                    cleaning = true;
                }
            }
        }
        if !links.owner.healthy() {
            cleaning = true;
        }
        if !links.peer.healthy() && !peer_gone {
            peer_gone = true;
            cleaning = true;
            if !finish_peer {
                links.fault();
            }
            if let Some(sweep) = &mut sweep {
                sweep.peer_gone();
            }
        }
        match workload.reap() {
            Ok(Some(status)) => links.notify(Kind::WorkloadExit, [status, workload.pid(), 0, 0]),
            Ok(None) => {}
            Err(_) => {
                links.fault();
                cleaning = true;
            }
        }
        if workload.reaped() && !cleaning && drain.empty(now) {
            // macOS retains its controlling-terminal association while S lives,
            // even after every slave FD closes. Retire empty sessions to expose
            // actual EOF; live descendants retain their bounded output window.
            cleaning = true;
        }
        if cleaning {
            cancellation.stop();
        }
        if cancellation.tick(&workload, now, cleaning) {
            links.fault();
            cleaning = true;
        }
        if cancellation.take_escalation_applied() {
            let frame = links.frame(Kind::Escalated, [workload.pid(), libc::SIGKILL, 0, 0]);
            links.owner.send(frame);
        }
        if cancellation.take_term_applied() {
            let frame = links.frame(Kind::SignalApplied, [workload.pid(), libc::SIGTERM, 7, 0]);
            links.owner.send(frame);
        }
        if cleaning && cancellation.empty() && sweep.is_none() {
            sweep = Some(Sweep::new(
                config.sid,
                pid,
                (!peer_gone).then_some(config.sid),
                host.as_raw_fd(),
                config.generation,
            ));
        }
        if let Some(sweep) = &mut sweep {
            match sweep.step(now) {
                Progress::Working => {}
                Progress::Retained => links.fault(),
                Progress::ReadyToRetire(members) => {
                    if !workload.settled() && !members.own && !members.peer {
                        // A final wait observation, not discovery, supplies W's status.
                    } else if members.own && members.peer && !peer_gone {
                        if !workload.settled() {
                            links.fault();
                        }
                        if crate::successor::handoff(
                            &mut links,
                            &host,
                            config.sid,
                            pid,
                            config.generation,
                        )
                        .is_err()
                        {
                            links.fault();
                            sweep.retain_for_retry(now);
                        }
                    } else if members.peer && !peer_gone {
                        finish_peer = true;
                        let frame = links.frame(Kind::Finish, [0; 4]);
                        links.peer.send(frame);
                    } else {
                        if !workload.settled() {
                            links.fault();
                        }
                        links.notify(Kind::Retiring, [pid, 0, 0, 0]);
                        links.flush();
                        os::terminate_own_group();
                    }
                }
            }
        }
        links.flush();
        let mut extra = Vec::with_capacity(7);
        if let Some(fd) = workload.pollfd() {
            extra.push(fd);
        }
        cancellation.pollfds(&mut extra);
        let mut delay = cancellation.delay(now);
        if let Some(sweep) = &sweep {
            sweep.pollfds(&mut extra);
            delay = sweep.idle_delay(now);
        }
        if workload.needs_poll() {
            delay = 10;
        }
        if workload.reaped() && !cleaning {
            delay = drain.delay(now);
        }
        links.pause(&extra, delay);
    }
}
fn await_sentinel(links: &mut Links, sid: i32, pid: i32) -> bool {
    let deadline = Instant::now() + std::time::Duration::from_secs(5);
    let mut sentinel_ready = false;
    let mut execution_granted = false;
    loop {
        let (owner, peer) = links.inbox();
        for frame in owner.into_iter().flatten() {
            if frame.kind != Kind::Execute {
                return false;
            }
            execution_granted = true;
        }
        for frame in peer.into_iter().flatten() {
            match frame.kind {
                Kind::Hello if frame.values[..2] == [sid, pid] => sentinel_ready = true,
                Kind::Execute => execution_granted = true,
                _ => return false,
            }
        }
        if execution_granted && sentinel_ready {
            links.flush();
            return links.owner.healthy() && links.peer.healthy();
        }
        if !links.owner.healthy() || !links.peer.healthy() || Instant::now() >= deadline {
            return false;
        }
        links.pause(&[], 50);
    }
}
