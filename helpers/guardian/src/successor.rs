//! A temporary cleanup successor handles workload entrants in both helper groups.
use crate::{
    cleanup::{Progress, Sweep},
    links::Links,
    os,
    protocol::Kind,
};
use std::{
    io::{self, Read, Write},
    os::{
        fd::{AsRawFd, OwnedFd},
        unix::net::UnixStream,
    },
    time::Instant,
};

pub fn handoff(
    links: &mut Links,
    host: &OwnedFd,
    sid: i32,
    parent: i32,
    generation: u64,
) -> io::Result<()> {
    let (mut parked, monitor) = UnixStream::pair()?;
    let pid = os::fork()?;
    if pid == 0 {
        drop(parked);
        let result = begin(links, host, sid, parent, generation, monitor);
        // SAFETY: the fresh single-threaded helper fork must not run its copied
        // parent's destructors if successor setup fails. Its parent observes EOF.
        unsafe {
            libc::_exit(if result.is_ok() { 0 } else { 125 });
        }
    }
    drop(monitor);
    // The successor exclusively takes over this endpoint state, including any
    // partial record. The former owner keeps only its private liveness channel.
    // Copies of these Rust owners are never dropped after close_unrelated: this
    // branch exits solely by group-zero SIGKILL below.
    if os::close_unrelated(&[parked.as_raw_fd()]).is_err() {
        // A failed inventory cannot leave duplicate owner/peer endpoints alive
        // while parked: death closes every copy and wakes sentinel recovery.
        os::terminate_own_group();
    }
    let mut byte = [0];
    loop {
        match parked.read(&mut byte) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            _ => os::terminate_own_group(),
        }
    }
}
fn begin(
    links: &mut Links,
    host: &OwnedFd,
    sid: i32,
    parent: i32,
    generation: u64,
    mut monitor: UnixStream,
) -> io::Result<()> {
    os::own_group()?;
    os::close_unrelated(&[
        links.owner.fd(),
        links.peer.fd(),
        host.as_raw_fd(),
        monitor.as_raw_fd(),
    ])?;
    monitor.set_nonblocking(true)?;
    // SAFETY: read-only identity of this newly forked group leader.
    let own = unsafe { libc::getpid() };
    links.notify(Kind::Successor, [own, parent, sid, 0]);
    let mut acknowledged = false;
    let mut peer_gone = false;
    let mut parent_retiring = false;
    let mut finish_peer = false;
    let mut sweep = Sweep::new(sid, own, Some(sid), host.as_raw_fd(), generation);
    sweep.additional_peer(parent);
    loop {
        let now = Instant::now();
        let (_, messages) = links.inbox();
        for message in messages.into_iter().flatten() {
            match message.kind {
                Kind::Finish if message.values[0] == own => acknowledged = true,
                Kind::Retiring if finish_peer => {}
                Kind::Terminate
                | Kind::Kill
                | Kind::Release
                | Kind::Abort
                | Kind::Hello
                | Kind::Execute => {}
                _ => links.fault(),
            }
        }
        if !links.peer.healthy() && !peer_gone {
            if !finish_peer {
                links.fault();
            }
            peer_gone = true;
            acknowledged = true;
            // The sentinel's remaining group now becomes a cleanup candidate.
            sweep.peer_gone();
            if !parent_retiring {
                sweep.additional_peer(parent);
            }
        }
        if acknowledged {
            match sweep.step(now) {
                Progress::Working => {}
                Progress::Retained => links.fault(),
                Progress::ReadyToRetire(members) if !parent_retiring => {
                    // The former owner is parked in its populated group and
                    // kills its own group on this byte or successor EOF. S still
                    // owns recovery if this successor unexpectedly disappears.
                    match monitor.write(&[1]) {
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                        _ => {
                            parent_retiring = true;
                            sweep = Sweep::new(
                                sid,
                                own,
                                (!peer_gone).then_some(sid),
                                host.as_raw_fd(),
                                generation,
                            );
                        }
                    }
                    let _ = members;
                }
                Progress::ReadyToRetire(members) => {
                    if members.own && members.peer && !peer_gone {
                        if handoff(links, host, sid, own, generation).is_err() {
                            links.fault();
                            sweep.retain_for_retry(now);
                        }
                    } else if members.peer && !peer_gone {
                        finish_peer = true;
                        let frame = links.frame(Kind::Finish, [0; 4]);
                        links.peer.send(frame);
                    } else {
                        links.notify(Kind::Retiring, [own, 0, 0, 0]);
                        links.flush();
                        os::terminate_own_group();
                    }
                }
            }
        }
        let mut extra = Vec::with_capacity(2);
        sweep.pollfds(&mut extra);
        links.pause(
            &extra,
            if acknowledged {
                sweep.idle_delay(now)
            } else {
                50
            },
        );
    }
}
