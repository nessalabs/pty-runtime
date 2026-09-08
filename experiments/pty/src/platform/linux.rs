use serde_json::{json, Value};
use std::{
    io,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
};

pub const BACKEND: &str = "epoll";

pub struct Readiness {
    fd: OwnedFd,
    events: Vec<libc::epoll_event>,
    indices: Vec<usize>,
}

impl Readiness {
    pub fn new(endpoints: &[OwnedFd]) -> io::Result<Self> {
        let raw = unsafe { libc::epoll_create1(libc::EPOLL_CLOEXEC) };
        if raw < 0 {
            return Err(io::Error::last_os_error());
        }
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };
        for (index, endpoint) in endpoints.iter().enumerate() {
            let mut event = libc::epoll_event {
                events: (libc::EPOLLIN | libc::EPOLLHUP | libc::EPOLLERR) as u32,
                u64: index as u64,
            };
            if unsafe {
                libc::epoll_ctl(
                    fd.as_raw_fd(),
                    libc::EPOLL_CTL_ADD,
                    endpoint.as_raw_fd(),
                    &mut event,
                )
            } < 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(Self {
            fd,
            events: vec![libc::epoll_event { events: 0, u64: 0 }; endpoints.len()],
            indices: Vec::with_capacity(endpoints.len()),
        })
    }

    pub fn wait(&mut self) -> io::Result<&[usize]> {
        let n = unsafe {
            libc::epoll_wait(
                self.fd.as_raw_fd(),
                self.events.as_mut_ptr(),
                self.events.len() as i32,
                -1,
            )
        };
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        self.indices.clear();
        self.indices
            .extend(self.events[..n as usize].iter().map(|e| e.u64 as usize));
        Ok(&self.indices)
    }

    pub fn remove(&self, endpoint: &OwnedFd) {
        assert_eq!(
            unsafe {
                libc::epoll_ctl(
                    self.fd.as_raw_fd(),
                    libc::EPOLL_CTL_DEL,
                    endpoint.as_raw_fd(),
                    std::ptr::null_mut(),
                )
            },
            0
        );
    }
}

fn kb(text: &str, key: &str) -> Option<u64> {
    text.lines().find_map(|line| {
        let tail = line.strip_prefix(key)?;
        tail.split_whitespace()
            .next()?
            .parse::<u64>()
            .ok()
            .map(|n| n * 1024)
    })
}

pub fn memory() -> Value {
    let status = std::fs::read_to_string("/proc/self/status").unwrap();
    let rollup = std::fs::read_to_string("/proc/self/smaps_rollup").unwrap();
    let private = kb(&rollup, "Private_Clean:").unwrap() + kb(&rollup, "Private_Dirty:").unwrap();
    let threads = status
        .lines()
        .find_map(|l| l.strip_prefix("Threads:"))
        .unwrap()
        .trim()
        .parse::<usize>()
        .unwrap();
    json!({"rss_bytes": kb(&rollup,"Rss:").unwrap(),
        "pss_bytes": kb(&rollup,"Pss:").unwrap(), "private_bytes": private,
        "charged_footprint_bytes": null, "virtual_bytes": kb(&status,"VmSize:").unwrap(),
        "threads": threads, "memory_source": "proc-smaps-rollup"})
}
