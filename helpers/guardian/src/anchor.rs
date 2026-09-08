//! A short-lived member supplies group identity; only it sends group-zero signals.
use crate::{
    os,
    protocol::{Channel, Frame, Kind},
};
use std::{
    io,
    os::{fd::AsRawFd, unix::net::UnixStream},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum State {
    Joining,
    Pinned,
    Rejected,
    Finished,
}
pub struct Anchor {
    pid: libc::pid_t,
    pub group: libc::pid_t,
    pub state: State,
    pub term_applied: bool,
    channel: Channel,
    watch: Option<os::ExitWatch>,
    wanted: Option<Kind>,
    sent: Option<Kind>,
    generation: u64,
}
impl Anchor {
    pub fn start(
        group: libc::pid_t,
        sid: libc::pid_t,
        host: i32,
        foreground: bool,
        generation: u64,
    ) -> io::Result<Self> {
        let (parent, child) = UnixStream::pair()?;
        // Complete fallible parent transport setup before creating a child whose
        // ledger must be retained until wait observes its exit.
        let channel = Channel::new(parent, generation)?;
        let pid = os::fork()?;
        if pid == 0 {
            drop(channel);
            let result = member(child, group, sid, host, foreground, generation);
            // SAFETY: this is the dedicated anchor fork. Never run copied parent
            // destructors or flush inherited buffers while terminating it.
            unsafe {
                libc::_exit(if result.is_ok() { 0 } else { 124 });
            }
        }
        drop(child);
        // Watch creation can race a rejected anchor's exit; retain ownership and
        // use nonblocking wait as the fallback, never an external numeric signal.
        let watch = os::ExitWatch::new(pid).ok();
        Ok(Self {
            pid,
            group,
            state: State::Joining,
            term_applied: false,
            channel,
            watch,
            wanted: None,
            sent: None,
            generation,
        })
    }
    pub fn request(&mut self, kind: Kind) {
        if self.wanted != Some(Kind::Kill) {
            self.wanted = Some(kind);
        }
    }
    pub fn tick(&mut self) -> io::Result<()> {
        if self.state == State::Finished {
            return Ok(());
        }
        for _ in 0..8 {
            let Some(frame) = self.channel.receive()? else {
                break;
            };
            match frame.kind {
                Kind::Hello if frame.values[0] == 1 => self.state = State::Pinned,
                Kind::Hello => self.state = State::Rejected,
                Kind::SignalApplied
                    if self.state == State::Pinned
                        && matches!(self.sent, Some(Kind::Terminate | Kind::Kill))
                        && frame.values == [self.group, libc::SIGTERM, 0, 0] =>
                {
                    self.term_applied = true;
                }
                _ => return Err(io::ErrorKind::InvalidData.into()),
            }
        }
        if self.state == State::Pinned && self.wanted != self.sent {
            if let Some(kind) = self.wanted {
                self.channel
                    .enqueue(Frame::new(kind, self.generation, [0; 4]))?;
                self.sent = Some(kind);
            }
        }
        // A member's EOF policy also kills its actual group if its parent dies.
        // A failed socket cannot authorize a numeric replacement signal.
        let _ = self.channel.flush();
        if os::wait(self.pid)?.is_some() {
            self.state = State::Finished;
            self.watch = None;
        }
        Ok(())
    }
    pub fn pollfds(&self, out: &mut Vec<libc::pollfd>) {
        out.push(libc::pollfd {
            fd: self.channel.fd(),
            events: libc::POLLIN
                | if self.channel.wants_write() {
                    libc::POLLOUT
                } else {
                    0
                },
            revents: 0,
        });
        if let Some(watch) = &self.watch {
            out.push(libc::pollfd {
                fd: watch.fd(),
                events: libc::POLLIN,
                revents: 0,
            });
        }
    }
}
fn member(
    stream: UnixStream,
    group: libc::pid_t,
    sid: libc::pid_t,
    host: i32,
    foreground: bool,
    generation: u64,
) -> io::Result<()> {
    os::close_unrelated(&[stream.as_raw_fd(), host])?;
    os::ignore_supervision_signals()?;
    // SAFETY: the fresh child joins only itself. Post-join SID validation is
    // required even where the kernel checked it earlier; no other thread/parent
    // subsequently moves this trusted anchor's group membership.
    let joined = unsafe { libc::setpgid(0, group) } == 0;
    let verified = joined
        && unsafe { libc::getsid(0) } == sid
        && (!foreground || unsafe { libc::tcgetpgrp(host) == libc::getpgrp() });
    let mut cleanup = MemberCleanup(verified);
    let mut channel = Channel::new(stream, generation)?;
    channel.enqueue(Frame::new(
        Kind::Hello,
        generation,
        [i32::from(verified), 0, 0, 0],
    ))?;
    loop {
        channel.flush()?;
        if !verified && !channel.wants_write() {
            return Ok(());
        }
        if let Some(frame) = channel.receive()? {
            match frame.kind {
                Kind::Terminate => {
                    // SAFETY: verified current membership pins the actual group;
                    // the anchor ignores TERM and stays a member through grace.
                    if unsafe { libc::kill(0, libc::SIGTERM) } < 0 {
                        return Err(io::Error::last_os_error());
                    }
                    channel.enqueue(Frame::new(
                        Kind::SignalApplied,
                        generation,
                        [group, libc::SIGTERM, 0, 0],
                    ))?;
                }
                Kind::Kill => os::terminate_own_group(),
                Kind::Release => {
                    cleanup.0 = false;
                    return Ok(());
                }
                _ => return Err(io::ErrorKind::InvalidData.into()),
            }
        }
        if channel.eof() {
            if verified {
                os::terminate_own_group();
            }
            return Ok(());
        }
        let mut fds = [libc::pollfd {
            fd: channel.fd(),
            events: libc::POLLIN
                | if channel.wants_write() {
                    libc::POLLOUT
                } else {
                    0
                },
            revents: 0,
        }];
        os::pause(&mut fds, -1)?;
    }
}
struct MemberCleanup(bool);
impl Drop for MemberCleanup {
    fn drop(&mut self) {
        if self.0 {
            os::terminate_own_group();
        }
    }
}
