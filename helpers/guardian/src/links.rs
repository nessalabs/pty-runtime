//! Bounded, nonblocking status delivery remains separate from child reaping.
use crate::{
    os,
    protocol::{Channel, Frame, Kind},
};
use std::{io, os::unix::net::UnixStream};
pub struct Endpoint {
    channel: Channel,
    healthy: bool,
    writable: bool,
    sent: [Option<[i32; 4]>; 19],
    pending: [Option<Frame>; 19],
}
impl Endpoint {
    fn new(stream: UnixStream, generation: u64) -> io::Result<Self> {
        Ok(Self {
            channel: Channel::new(stream, generation)?,
            healthy: true,
            writable: true,
            sent: [None; 19],
            pending: [None; 19],
        })
    }
    pub fn send(&mut self, frame: Frame) {
        let index = frame.kind as usize;
        if !self.healthy || !self.writable || self.sent[index] == Some(frame.values) {
            return;
        }
        // Successor/Finish can carry a new bounded handoff generation. Only one
        // handoff is admitted until its peer acknowledges the current successor.
        if self.sent[index].is_some() && !matches!(frame.kind, Kind::Successor | Kind::Finish) {
            return;
        }
        self.pending[index] = Some(frame);
        self.sent[index] = Some(frame.values);
    }
    fn receive(&mut self) -> [Option<Frame>; 8] {
        let mut messages = [None; 8];
        if !self.healthy {
            return messages;
        }
        for message in &mut messages {
            match self.channel.receive() {
                Ok(Some(frame)) => *message = Some(frame),
                Ok(None) => break,
                Err(_) => {
                    self.healthy = false;
                    break;
                }
            }
        }
        if self.channel.eof() {
            self.healthy = false;
        }
        messages
    }
    fn flush(&mut self) {
        if !self.healthy || !self.writable {
            return;
        }
        for pending in &mut self.pending {
            if let Some(frame) = *pending {
                if self.channel.enqueue(frame).is_err() {
                    break;
                }
                *pending = None;
            }
        }
        if self.channel.flush().is_err() {
            // Peer exit can race a pending write while final status/Retiring
            // records remain readable. Drain those before classifying EOF.
            self.writable = false;
        }
    }
    fn pollfd(&self) -> libc::pollfd {
        libc::pollfd {
            fd: if self.healthy { self.channel.fd() } else { -1 },
            events: libc::POLLIN
                | if self.writable && self.channel.wants_write() {
                    libc::POLLOUT
                } else {
                    0
                },
            revents: 0,
        }
    }
    pub fn healthy(&self) -> bool {
        self.healthy
    }
    pub fn fd(&self) -> i32 {
        self.channel.fd()
    }
}
pub struct Links {
    pub owner: Endpoint,
    pub peer: Endpoint,
    generation: u64,
}
impl Links {
    pub fn new(owner: UnixStream, peer: UnixStream, generation: u64) -> io::Result<Self> {
        Ok(Self {
            owner: Endpoint::new(owner, generation)?,
            peer: Endpoint::new(peer, generation)?,
            generation,
        })
    }
    pub fn frame(&self, kind: Kind, values: [i32; 4]) -> Frame {
        Frame::new(kind, self.generation, values)
    }
    pub fn notify(&mut self, kind: Kind, values: [i32; 4]) {
        let frame = self.frame(kind, values);
        self.owner.send(frame);
        self.peer.send(frame);
    }
    pub fn fault(&mut self) {
        self.notify(Kind::Fault, [0; 4]);
    }
    pub fn inbox(&mut self) -> ([Option<Frame>; 8], [Option<Frame>; 8]) {
        (self.owner.receive(), self.peer.receive())
    }
    pub fn flush(&mut self) {
        self.owner.flush();
        self.peer.flush();
    }
    pub fn pollfds(&self) -> Vec<libc::pollfd> {
        vec![self.owner.pollfd(), self.peer.pollfd()]
    }
    pub fn pause(&mut self, extra: &[libc::pollfd], milliseconds: i32) {
        self.flush();
        let mut fds = self.pollfds();
        fds.extend_from_slice(extra);
        if os::pause(&mut fds, milliseconds).is_err() {
            self.fault();
        }
    }
}
