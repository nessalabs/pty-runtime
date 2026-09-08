use std::{
    fs::{File, ReadDir},
    io::{self, Read},
};
pub struct Inventory(ReadDir);
impl Inventory {
    pub fn new() -> io::Result<Self> {
        std::fs::read_dir("/proc").map(Self)
    }
    pub fn next(&mut self) -> Option<io::Result<libc::pid_t>> {
        loop {
            let entry = match self.0.next()? {
                Ok(entry) => entry,
                Err(error) => return Some(Err(error)),
            };
            if let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<libc::pid_t>().ok())
            {
                if pid > 0 {
                    return Some(Ok(pid));
                }
            }
        }
    }
}
pub fn live(pid: libc::pid_t) -> io::Result<bool> {
    let mut file = File::open(format!("/proc/{pid}/stat"))?;
    let mut bytes = [0; 4096];
    let count = file.read(&mut bytes)?;
    if count == bytes.len() {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let end = bytes[..count]
        .iter()
        .rposition(|byte| *byte == b')')
        .ok_or(io::ErrorKind::InvalidData)?;
    if bytes.get(end + 1) != Some(&b' ') {
        return Err(io::ErrorKind::InvalidData.into());
    }
    match bytes.get(end + 2) {
        Some(b'Z' | b'X') => Ok(false),
        Some(b'R' | b'S' | b'D' | b'T' | b't' | b'I' | b'P') => Ok(true),
        _ => Err(io::ErrorKind::InvalidData.into()),
    }
}
