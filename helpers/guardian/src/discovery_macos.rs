use std::io;
const MAX_PIDS: usize = 32768;
pub struct Inventory {
    pids: Box<[libc::pid_t]>,
    index: usize,
    count: usize,
}
impl Inventory {
    pub fn new() -> io::Result<Self> {
        let mut pids = vec![0; MAX_PIDS].into_boxed_slice();
        // SAFETY: the fixed initialized array is writable for exactly the
        // supplied byte capacity. A full/truncated result is rejected below.
        let count = unsafe {
            libc::proc_listallpids(
                pids.as_mut_ptr().cast(),
                std::mem::size_of_val(&*pids) as i32,
            )
        };
        if count <= 0 {
            return Err(io::Error::last_os_error());
        }
        if count as usize >= MAX_PIDS {
            return Err(io::Error::new(
                io::ErrorKind::OutOfMemory,
                "process inventory capacity",
            ));
        }
        Ok(Self {
            pids,
            index: 0,
            count: count as usize,
        })
    }
    pub fn next(&mut self) -> Option<io::Result<libc::pid_t>> {
        if self.index == self.count {
            return None;
        }
        let pid = self.pids[self.index];
        self.index += 1;
        Some(Ok(pid))
    }
}
pub fn live(pid: libc::pid_t) -> io::Result<bool> {
    // SAFETY: proc_bsdinfo is a plain C output record; zero is valid initialized
    // storage and proc_pidinfo writes no more than the supplied exact length.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of_val(&info) as i32;
    let count = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            size,
        )
    };
    if count != size {
        return Err(io::Error::last_os_error());
    }
    Ok(info.pbi_status != libc::SZOMB)
}
