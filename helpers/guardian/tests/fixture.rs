//! Real descendant fixtures; this binary is never embedded in the runtime.
use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
};
fn member(group: i32) -> std::io::Result<i32> {
    let (mut parent, mut child) = UnixStream::pair()?;
    // SAFETY: this dedicated fixture process is single-threaded and owns all
    // descriptors and state inherited by its child.
    let pid = unsafe { libc::fork() };
    if pid < 0 {
        return Err(std::io::Error::last_os_error());
    }
    if pid == 0 {
        drop(parent);
        // SAFETY: the fixture changes only its own group and dispositions.
        unsafe {
            if libc::setpgid(0, group) < 0 {
                libc::_exit(120);
            }
            libc::signal(libc::SIGHUP, libc::SIG_IGN);
            libc::signal(libc::SIGTERM, libc::SIG_IGN);
        }
        let _ = child.write_all(&[1]);
        drop(child);
        loop {
            // SAFETY: pause only suspends this isolated fixture until a signal.
            unsafe {
                libc::pause();
            }
        }
    }
    drop(child);
    parent.read_exact(&mut [0])?;
    Ok(pid)
}
fn main() -> std::io::Result<()> {
    let mode = std::env::args()
        .nth(1)
        .ok_or(std::io::ErrorKind::InvalidInput)?;
    // SAFETY: read-only process identity queries in this fixture.
    let (sentinel, guardian) = unsafe { (libc::getsid(0), libc::getppid()) };
    let mut children = Vec::new();
    if mode == "sentinel" || mode == "both" {
        children.push(member(sentinel)?);
    }
    if mode == "guardian" || mode == "both" {
        children.push(member(guardian)?);
    }
    print!("ready");
    for pid in children {
        print!(" {pid}");
    }
    println!();
    std::io::stdout().flush()?;
    let mut byte = [0];
    let _ = std::io::stdin().read(&mut byte)?;
    Ok(())
}
