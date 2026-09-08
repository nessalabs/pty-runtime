//! Normal TERM/KILL targets the actual workload plus pinned root/foreground groups.
use crate::{
    anchor::{Anchor, State},
    protocol::Kind,
    workload::Workload,
};
use std::time::{Duration, Instant};
pub struct Cancellation {
    sid: i32,
    guardian: i32,
    host: i32,
    generation: u64,
    anchors: Vec<Anchor>,
    requested: Option<Kind>,
    applied: Option<Kind>,
    term_at: Option<Instant>,
    grace: Duration,
    resample: bool,
    retry_at: Instant,
    term_direct: bool,
    term_targets: Option<[i32; 2]>,
    term_groups: [bool; 2],
    term_reported: bool,
    escalation_applied: bool,
}
impl Cancellation {
    pub fn new(sid: i32, guardian: i32, host: i32, generation: u64, grace: Duration) -> Self {
        Self {
            sid,
            guardian,
            host,
            generation,
            anchors: Vec::with_capacity(2),
            requested: None,
            applied: None,
            term_at: None,
            grace,
            resample: false,
            retry_at: Instant::now(),
            term_direct: false,
            term_targets: None,
            term_groups: [false; 2],
            term_reported: false,
            escalation_applied: false,
        }
    }
    pub fn request(&mut self, kind: Kind) {
        if self.requested != Some(Kind::Kill) {
            self.requested = Some(kind);
        }
    }
    pub fn stop(&mut self) {
        self.request(Kind::Kill);
        self.resample = false;
    }
    pub fn tick(&mut self, workload: &Workload, now: Instant, cleaning: bool) -> bool {
        let mut failed = false;
        for anchor in &mut self.anchors {
            if anchor.tick().is_err() {
                failed = true;
            }
            if anchor.term_applied {
                if let Some(targets) = self.term_targets {
                    for (index, group) in targets.into_iter().enumerate() {
                        self.term_groups[index] |= group == anchor.group;
                    }
                }
            }
        }
        self.anchors
            .retain(|anchor| anchor.state != State::Finished);
        if self
            .term_at
            .is_some_and(|start| now.duration_since(start) >= self.grace)
        {
            self.request(Kind::Kill);
        }
        if self.requested != self.applied {
            if let Some(kind) = self.requested {
                match workload.signal(if kind == Kind::Kill {
                    libc::SIGKILL
                } else {
                    libc::SIGTERM
                }) {
                    Ok(applied) if kind == Kind::Terminate => self.term_direct = applied,
                    Ok(true)
                        if kind == Kind::Kill
                            && self.applied == Some(Kind::Terminate)
                            && !cleaning =>
                    {
                        self.escalation_applied = true;
                    }
                    Ok(_) => (),
                    Err(_) => failed = true,
                }
                for anchor in &mut self.anchors {
                    anchor.request(kind);
                }
                if kind == Kind::Terminate {
                    self.term_at = Some(now);
                }
                self.resample = true;
                self.applied = Some(kind);
            }
        }
        if cleaning {
            self.resample = false;
            return failed;
        }
        if self.resample && now >= self.retry_at {
            // SAFETY: this only discovers a hint; the member anchor must verify
            // its actual session and current foreground before any signal.
            let foreground = unsafe { libc::tcgetpgrp(self.host) };
            if foreground < 0 {
                return true;
            }
            if foreground == self.sid || foreground == self.guardian {
                return true;
            }
            if self.requested == Some(Kind::Terminate) && self.term_targets.is_none() {
                self.term_targets = Some([workload.pid(), foreground]);
            }
            let targets = [(workload.pid(), false), (foreground, true)];
            for (group, foreground_only) in targets {
                if group <= 0 || self.anchors.iter().any(|anchor| anchor.group == group) {
                    continue;
                }
                if self.anchors.len() == 2 {
                    break;
                }
                match Anchor::start(group, self.sid, self.host, foreground_only, self.generation) {
                    Ok(mut anchor) => {
                        if let Some(kind) = self.requested {
                            anchor.request(kind);
                        }
                        self.anchors.push(anchor);
                    }
                    Err(_) => failed = true,
                }
            }
            // At KILL a fresh sample follows retirement of old anchors. Under
            // churn finite work continues instead of an unsafe numeric fallback.
            self.resample = self.requested == Some(Kind::Kill) && !workload.reaped();
            self.retry_at = now + Duration::from_millis(10);
        }
        failed
    }
    pub fn take_escalation_applied(&mut self) -> bool {
        std::mem::take(&mut self.escalation_applied)
    }
    /// Acknowledges actual direct and verified root/foreground TERM syscalls,
    /// never mere queueing or disappearance. Missing/rejected targets yield no sample.
    pub fn take_term_applied(&mut self) -> bool {
        if !self.term_reported
            && self.term_direct
            && self.term_targets.is_some()
            && self.term_groups.iter().all(|applied| *applied)
        {
            self.term_reported = true;
            true
        } else {
            false
        }
    }
    pub fn empty(&self) -> bool {
        self.anchors.is_empty()
    }
    pub fn pollfds(&self, out: &mut Vec<libc::pollfd>) {
        for anchor in &self.anchors {
            anchor.pollfds(out);
        }
    }
    pub fn delay(&self, now: Instant) -> i32 {
        if !self.anchors.is_empty() || self.resample {
            return 10;
        }
        self.term_at
            .filter(|_| self.requested != Some(Kind::Kill))
            .map_or(-1, |start| {
                (start + self.grace)
                    .saturating_duration_since(now)
                    .as_millis()
                    .min(i32::MAX as u128) as i32
            })
    }
}
