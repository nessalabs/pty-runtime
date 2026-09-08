//! Fresh executable boundary for per-session supervision and verified group control.
mod anchor;
mod cancellation;
mod cleanup;
mod discovery;
mod drain;
mod guardian;
mod links;
mod os;
#[path = "../../../scripts/guardian/protocol.rs"]
mod protocol;
mod sentinel;
mod successor;
mod workload;
use std::{
    ffi::OsString,
    io,
    os::{fd::AsRawFd, unix::net::UnixStream},
    time::Duration,
};
pub struct Config {
    generation: u64,
    sid: i32,
    grace: Duration,
    arguments: Vec<OsString>,
}
fn main() {
    if start().is_err() {
        // Startup diagnostics deliberately exclude command/environment data.
        std::process::exit(125);
    }
}
fn start() -> io::Result<()> {
    let mut arguments = std::env::args_os().skip(1);
    if arguments.next().as_deref() != Some(std::ffi::OsStr::new("--pty-runtime-guardian-v1")) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let generation = number(arguments.next())?;
    let grace = number(arguments.next())?;
    if generation == 0 || grace > 86_400_000 {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let arguments: Vec<_> = arguments.collect();
    if arguments.is_empty()
        || arguments.len() > 129
        || arguments
            .iter()
            .map(|argument| argument.len())
            .sum::<usize>()
            > 65536
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    os::close_unrelated(&[3, 4, 5, 6])?;
    let owner_s = UnixStream::from(os::owned(3)?);
    let owner_g = UnixStream::from(os::owned(4)?);
    let host = os::owned(5)?;
    let slave = os::owned(6)?;
    os::ignore_supervision_signals()?;
    let sid = os::session(slave.as_raw_fd())?;
    let (peer_s, peer_g) = UnixStream::pair()?;
    let pid = os::fork()?;
    if pid == 0 {
        drop(owner_s);
        drop(peer_s);
        let links = links::Links::new(owner_g, peer_g, generation)?;
        let config = Config {
            generation,
            sid,
            grace: Duration::from_millis(grace),
            arguments,
        };
        let result = guardian::run(links, host, slave, config);
        // SAFETY: this fork is the dedicated guardian; never return through S's
        // copied stack/destructors or flush copied host buffers.
        unsafe {
            libc::_exit(if result.is_ok() { 0 } else { 125 });
        }
    }
    drop(owner_g);
    drop(peer_g);
    drop(slave);
    let links = links::Links::new(owner_s, peer_s, generation)?;
    sentinel::run(links, host, sid, pid, generation)
}
fn number(value: Option<OsString>) -> io::Result<u64> {
    value
        .and_then(|value| value.to_str().and_then(|value| value.parse().ok()))
        .ok_or_else(|| io::ErrorKind::InvalidInput.into())
}
