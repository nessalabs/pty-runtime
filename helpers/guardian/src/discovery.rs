//! Process metadata is a discovery hint, never signalling authority.
use std::io;
#[cfg(target_os = "linux")]
#[path = "discovery_linux.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "discovery_macos.rs"]
mod platform;

pub enum Observation {
    OutsideOrGone,
    Member {
        pid: libc::pid_t,
        group: libc::pid_t,
    },
    Unknown,
}
pub struct Discovery {
    platform: platform::Inventory,
    sid: libc::pid_t,
}
impl Discovery {
    pub fn new(sid: libc::pid_t) -> io::Result<Self> {
        Ok(Self {
            platform: platform::Inventory::new()?,
            sid,
        })
    }
    pub fn next(&mut self) -> Option<Observation> {
        let pid = match self.platform.next()? {
            Ok(pid) => pid,
            Err(_) => return Some(Observation::Unknown),
        };
        if pid <= 0 {
            return Some(Observation::OutsideOrGone);
        }
        // SAFETY: getsid is a read-only metadata query. A number obtained here
        // never authorizes signalling; an anchor independently verifies itself.
        let sid = unsafe { libc::getsid(pid) };
        if sid < 0 {
            return Some(gone_or_unknown(io::Error::last_os_error()));
        }
        if sid != self.sid {
            return Some(Observation::OutsideOrGone);
        }
        match platform::live(pid) {
            Ok(false) => Some(Observation::OutsideOrGone),
            Err(error) => Some(gone_or_unknown(error)),
            Ok(true) => {
                // SAFETY: getpgid only reads metadata; same-session membership
                // and group lifetime are established later inside the anchor.
                let group = unsafe { libc::getpgid(pid) };
                Some(if group < 0 {
                    gone_or_unknown(io::Error::last_os_error())
                } else {
                    Observation::Member { pid, group }
                })
            }
        }
    }
}
fn gone_or_unknown(error: io::Error) -> Observation {
    if matches!(error.raw_os_error(), Some(libc::ESRCH | libc::ENOENT)) {
        Observation::OutsideOrGone
    } else {
        Observation::Unknown
    }
}
