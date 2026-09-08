//! A non-destructive census lets an empty controlling session reach real EOF.
use crate::discovery::{Discovery, Observation};
use std::time::{Duration, Instant};

pub struct Drain {
    sid: i32,
    guardian: i32,
    scan: Option<Discovery>,
    retry_at: Instant,
}
impl Drain {
    pub fn new(sid: i32, guardian: i32) -> Self {
        Self {
            sid,
            guardian,
            scan: None,
            retry_at: Instant::now(),
        }
    }
    pub fn empty(&mut self, now: Instant) -> bool {
        if now < self.retry_at {
            return false;
        }
        if self.scan.is_none() {
            self.scan = Discovery::new(self.sid).ok();
            if self.scan.is_none() {
                self.retry(now);
                return false;
            }
        }
        for _ in 0..128 {
            match self.scan.as_mut().and_then(Discovery::next) {
                Some(Observation::OutsideOrGone) => {}
                Some(Observation::Member { pid, .. })
                    if pid == self.sid || pid == self.guardian => {}
                Some(Observation::Member { .. } | Observation::Unknown) => {
                    self.retry(now);
                    return false;
                }
                None => {
                    self.scan = None;
                    return true;
                }
            }
        }
        false
    }
    pub fn delay(&self, now: Instant) -> i32 {
        if self.scan.is_some() || now >= self.retry_at {
            0
        } else {
            self.retry_at.duration_since(now).as_millis().min(10) as i32
        }
    }
    fn retry(&mut self, now: Instant) {
        self.scan = None;
        self.retry_at = now + Duration::from_millis(10);
    }
}
