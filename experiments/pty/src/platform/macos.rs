use serde_json::{json, Value};
use std::{
    io,
    mem::{size_of, zeroed},
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
};

pub const BACKEND: &str = "kqueue";

pub struct Readiness {
    fd: OwnedFd,
    events: Vec<libc::kevent>,
    indices: Vec<usize>,
}

impl Readiness {
    /// An empty reactor whose membership changes while the worker runs. Used only
    /// by the handoff fixture; the fixed-placement cases keep using `new`.
    pub fn with_capacity(capacity: usize) -> io::Result<Self> {
        let raw = unsafe { libc::kqueue() };
        if raw < 0 {
            return Err(io::Error::last_os_error());
        }
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };
        assert_eq!(
            unsafe { libc::fcntl(raw, libc::F_SETFD, libc::FD_CLOEXEC) },
            0
        );
        Ok(Self {
            fd,
            events: (0..capacity.max(1)).map(|_| unsafe { zeroed() }).collect(),
            indices: Vec::with_capacity(capacity.max(1)),
        })
    }

    /// Register one endpoint under `index`. Registration is an ownership change,
    /// so the caller must have stopped every previous reader of that descriptor.
    pub fn add(&self, endpoint: &OwnedFd, index: usize) -> io::Result<()> {
        let change = libc::kevent {
            ident: endpoint.as_raw_fd() as usize,
            filter: libc::EVFILT_READ,
            flags: libc::EV_ADD | libc::EV_ENABLE,
            fflags: 0,
            data: 0,
            udata: index as *mut _,
        };
        if unsafe {
            libc::kevent(
                self.fd.as_raw_fd(),
                &change,
                1,
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
            )
        } < 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    pub fn new(endpoints: &[OwnedFd]) -> io::Result<Self> {
        let raw = unsafe { libc::kqueue() };
        if raw < 0 {
            return Err(io::Error::last_os_error());
        }
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };
        assert_eq!(
            unsafe { libc::fcntl(raw, libc::F_SETFD, libc::FD_CLOEXEC) },
            0
        );
        let changes: Vec<_> = endpoints
            .iter()
            .enumerate()
            .map(|(i, e)| libc::kevent {
                ident: e.as_raw_fd() as usize,
                filter: libc::EVFILT_READ,
                flags: libc::EV_ADD | libc::EV_ENABLE,
                fflags: 0,
                data: 0,
                udata: i as *mut _,
            })
            .collect();
        if unsafe {
            libc::kevent(
                raw,
                changes.as_ptr(),
                changes.len() as i32,
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
            )
        } < 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            fd,
            events: (0..endpoints.len()).map(|_| unsafe { zeroed() }).collect(),
            indices: Vec::with_capacity(endpoints.len()),
        })
    }

    pub fn wait(&mut self) -> io::Result<&[usize]> {
        let n = unsafe {
            libc::kevent(
                self.fd.as_raw_fd(),
                std::ptr::null(),
                0,
                self.events.as_mut_ptr(),
                self.events.len() as i32,
                std::ptr::null(),
            )
        };
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        self.indices.clear();
        self.indices
            .extend(self.events[..n as usize].iter().map(|e| e.udata as usize));
        Ok(&self.indices)
    }

    pub fn remove(&self, endpoint: &OwnedFd) {
        let change = libc::kevent {
            ident: endpoint.as_raw_fd() as usize,
            filter: libc::EVFILT_READ,
            flags: libc::EV_DELETE,
            fflags: 0,
            data: 0,
            udata: std::ptr::null_mut(),
        };
        assert_eq!(
            unsafe {
                libc::kevent(
                    self.fd.as_raw_fd(),
                    &change,
                    1,
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null(),
                )
            },
            0
        );
    }
}

pub fn memory() -> Value {
    unsafe {
        let mut task: libc::proc_taskinfo = zeroed();
        assert_eq!(
            libc::proc_pidinfo(
                libc::getpid(),
                libc::PROC_PIDTASKINFO,
                0,
                &mut task as *mut _ as *mut _,
                size_of::<libc::proc_taskinfo>() as i32
            ),
            size_of::<libc::proc_taskinfo>() as i32
        );
        let mut usage: libc::rusage_info_v4 = zeroed();
        assert_eq!(
            libc::proc_pid_rusage(
                libc::getpid(),
                libc::RUSAGE_INFO_V4,
                &mut usage as *mut _ as *mut _
            ),
            0
        );
        json!({"rss_bytes": task.pti_resident_size, "pss_bytes": null,
            "private_bytes": null, "charged_footprint_bytes": usage.ri_phys_footprint,
            "virtual_bytes": task.pti_virtual_size, "threads": task.pti_threadnum,
            "memory_source": "proc-pid-rusage"})
    }
}
