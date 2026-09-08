//! The session leader retains independent recovery ownership if G disappears.
use crate::{
    cleanup::{Progress, Sweep},
    links::Links,
    os,
    protocol::Kind,
};
use std::{
    os::fd::{AsRawFd, OwnedFd},
    time::Instant,
};
pub fn run(mut links: Links, host: OwnedFd, sid: i32, guardian: i32, generation: u64) -> ! {
    let mut watch = os::ExitWatch::new(guardian).ok();
    let mut guardian_reaped = false;
    let mut retiring = false;
    let mut startup_failed = false;
    let mut successor: Option<(i32, Option<os::ExitWatch>)> = None;
    let mut sweep: Option<Sweep> = None;
    links.notify(Kind::Hello, [sid, guardian, 0, 0]);
    loop {
        let (owner, peer) = links.inbox();
        for frame in owner.into_iter().flatten() {
            match frame.kind {
                Kind::Terminate | Kind::Kill | Kind::Release | Kind::Abort | Kind::Execute => {
                    links.peer.send(frame)
                }
                _ => {
                    links.fault();
                    let frame = links.frame(Kind::Abort, [0; 4]);
                    links.peer.send(frame);
                }
            }
        }
        for frame in peer.into_iter().flatten() {
            match frame.kind {
                Kind::Hello if frame.values[..2] == [sid, guardian] => {}
                Kind::Started
                    if frame.values[0] > 0 && frame.values[1..] == [guardian, sid, sid] =>
                {
                    let admitted = links.frame(Kind::Admitted, frame.values);
                    links.owner.send(admitted);
                }
                Kind::WorkloadExit | Kind::Fault => links.owner.send(frame),
                Kind::StartFailed => {
                    startup_failed = true;
                    links.owner.send(frame);
                }
                Kind::Retiring => retiring = true,
                Kind::Successor
                    if frame.values[0] > 0
                        && frame.values[2] == sid
                        && frame.values[1]
                            == successor.as_ref().map_or(guardian, |(pid, _)| *pid) =>
                {
                    successor = Some((frame.values[0], os::ExitWatch::new(frame.values[0]).ok()));
                    links.owner.send(frame);
                    let ack = links.frame(Kind::Finish, [frame.values[0], 0, 0, 0]);
                    links.peer.send(ack);
                }
                Kind::Finish => {
                    // G has verified its own group has no workload members and
                    // remains the recovery owner while this populated group dies.
                    links.notify(Kind::Retiring, [sid, 0, 0, 0]);
                    links.flush();
                    os::terminate_own_group();
                }
                _ => {
                    links.fault();
                    let frame = links.frame(Kind::Abort, [0; 4]);
                    links.peer.send(frame);
                }
            }
        }
        if !links.owner.healthy() {
            let frame = links.frame(Kind::Abort, [0; 4]);
            links.peer.send(frame);
        }
        if !guardian_reaped {
            match os::wait(guardian) {
                Ok(Some(_)) => {
                    guardian_reaped = true;
                    watch = None;
                }
                Ok(None) => {}
                Err(_) => links.fault(),
            }
        }
        if !links.peer.healthy() && sweep.is_none() {
            if !retiring && !startup_failed {
                links.fault();
            }
            sweep = Some(Sweep::new(sid, sid, None, host.as_raw_fd(), generation));
        }
        if let Some(sweep) = &mut sweep {
            match sweep.step(Instant::now()) {
                Progress::Retained => links.fault(),
                Progress::ReadyToRetire(_) if guardian_reaped => {
                    links.notify(Kind::Retiring, [sid, 0, 0, 0]);
                    links.flush();
                    os::terminate_own_group();
                }
                _ => {}
            }
        }
        let mut extra = Vec::with_capacity(3);
        if let Some(watch) = &watch {
            extra.push(libc::pollfd {
                fd: watch.fd(),
                events: libc::POLLIN,
                revents: 0,
            });
        }
        let mut delay = if watch.is_none() && !guardian_reaped {
            10
        } else {
            -1
        };
        if let Some((_, Some(watch))) = &successor {
            extra.push(libc::pollfd {
                fd: watch.fd(),
                events: libc::POLLIN,
                revents: 0,
            });
        }
        if let Some(sweep) = &sweep {
            sweep.pollfds(&mut extra);
            delay = sweep.idle_delay(Instant::now());
        }
        links.pause(&extra, delay);
    }
}
