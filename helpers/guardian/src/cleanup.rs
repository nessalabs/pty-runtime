//! A surviving helper retains its SID while bounded discovery work continues.
use crate::{
    anchor::{Anchor, State},
    discovery::{Discovery, Observation},
    protocol::Kind,
};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Default)]
pub struct ReservedMembers {
    pub own: bool,
    pub peer: bool,
}
pub enum Progress {
    Working,
    Retained,
    ReadyToRetire(ReservedMembers),
}
pub struct Sweep {
    sid: libc::pid_t,
    own: libc::pid_t,
    peers: [Option<libc::pid_t>; 2],
    host: i32,
    generation: u64,
    scan: Option<Discovery>,
    anchor: Option<Anchor>,
    uncertain: bool,
    acted: bool,
    reserved: ReservedMembers,
    retry_at: Instant,
}
impl Sweep {
    pub fn new(
        sid: libc::pid_t,
        own: libc::pid_t,
        peer: Option<libc::pid_t>,
        host: i32,
        generation: u64,
    ) -> Self {
        Self {
            sid,
            own,
            peers: [peer, None],
            host,
            generation,
            scan: None,
            anchor: None,
            uncertain: false,
            acted: false,
            reserved: ReservedMembers::default(),
            retry_at: Instant::now(),
        }
    }
    pub fn peer_gone(&mut self) {
        self.peers = [None; 2];
        // A topology transition invalidates this scan but is not a metadata
        // failure. Restart so the former peer group is explicitly included.
        self.scan = None;
        self.uncertain = false;
        self.acted = false;
        self.reserved = ReservedMembers::default();
        self.retry_at = Instant::now();
    }
    pub fn additional_peer(&mut self, pid: i32) {
        self.peers[1] = Some(pid);
    }
    pub fn retain_for_retry(&mut self, now: Instant) {
        let _ = self.retry(now);
    }
    pub fn step(&mut self, now: Instant) -> Progress {
        if now < self.retry_at {
            return Progress::Working;
        }
        if let Some(anchor) = &mut self.anchor {
            if anchor.tick().is_err() {
                self.uncertain = true;
                // Keep the child ledger/channel: failure is not permission to
                // forget a live anchor or issue a cached numeric-group signal.
                self.retry_at = now + Duration::from_millis(10);
                return Progress::Retained;
            }
            if anchor.state != State::Finished {
                return Progress::Working;
            }
            self.anchor = None;
        }
        if self.scan.is_none() {
            match Discovery::new(self.sid) {
                Ok(scan) => self.scan = Some(scan),
                Err(_) => return self.retry(now),
            }
        }
        for _ in 0..128 {
            let observation = self.scan.as_mut().and_then(Discovery::next);
            match observation {
                Some(Observation::OutsideOrGone) => {}
                Some(Observation::Unknown) => self.uncertain = true,
                Some(Observation::Member { pid, group }) => {
                    if pid == self.own || self.peers.contains(&Some(pid)) {
                        continue;
                    }
                    if group == self.own {
                        self.reserved.own = true;
                        continue;
                    }
                    if self.peers.contains(&Some(group)) {
                        self.reserved.peer = true;
                        continue;
                    }
                    self.acted = true;
                    match Anchor::start(group, self.sid, self.host, false, self.generation) {
                        Ok(mut anchor) => {
                            anchor.request(Kind::Kill);
                            self.anchor = Some(anchor);
                        }
                        Err(_) => self.uncertain = true,
                    }
                    return Progress::Working;
                }
                None => {
                    self.scan = None;
                    if self.uncertain {
                        return self.retry(now);
                    }
                    if self.acted {
                        self.acted = false;
                        self.reserved = ReservedMembers::default();
                        self.retry_at = now + Duration::from_millis(10);
                        return Progress::Working;
                    }
                    return Progress::ReadyToRetire(self.reserved);
                }
            }
        }
        Progress::Working
    }
    fn retry(&mut self, now: Instant) -> Progress {
        self.scan = None;
        self.uncertain = false;
        self.acted = false;
        self.reserved = ReservedMembers::default();
        self.retry_at = now + Duration::from_millis(50);
        Progress::Retained
    }
    pub fn pollfds(&self, out: &mut Vec<libc::pollfd>) {
        if let Some(anchor) = &self.anchor {
            anchor.pollfds(out);
        }
    }
    pub fn idle_delay(&self, now: Instant) -> i32 {
        if self.anchor.is_some() {
            return 10;
        }
        self.retry_at
            .saturating_duration_since(now)
            .as_millis()
            .min(50) as i32
    }
}
