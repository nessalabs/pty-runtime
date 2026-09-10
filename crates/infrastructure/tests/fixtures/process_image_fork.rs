//! The real helper constructor must remain executable across an unrelated fork.
use super::{HelperImage, IMAGE};
use std::{
    fs,
    io::{self, Read, Write},
    os::{
        fd::AsRawFd,
        unix::{net::UnixStream, process::CommandExt},
    },
    process::{Child, Command, ExitStatus, Stdio},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

struct ChildGuard(Child);
impl ChildGuard {
    fn finish(&mut self) -> io::Result<ExitStatus> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.0.try_wait()? {
                return Ok(status);
            }
            if Instant::now() >= deadline {
                return Err(io::ErrorKind::TimedOut.into());
            }
            thread::sleep(Duration::from_millis(1));
        }
    }
}
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

// Command::spawn has not returned while its child is blocked in pre_exec.
// Keep the parent-side control and join owner so every assertion can unwind safely.
struct BlockedFork {
    control: UnixStream,
    launch: Option<JoinHandle<io::Result<Child>>>,
}
impl BlockedFork {
    fn start() -> io::Result<Self> {
        let (control, child_control) = UnixStream::pair()?;
        control.set_read_timeout(Some(Duration::from_secs(5)))?;
        control.set_write_timeout(Some(Duration::from_secs(1)))?;
        let launch = thread::spawn(move || {
            let fd = child_control.as_raw_fd();
            let mut command = Command::new("/usr/bin/true");
            command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            // SAFETY: after fork this closure uses only write/poll/read/_exit on
            // an inherited live socket and fixed stack bytes. There is no Rust
            // allocation, logging, locking or owner destruction. poll bounds a lost-parent
            // gate to ten seconds; normal release comes from the parent socket.
            unsafe {
                command.pre_exec(move || {
                    let ready = *b"R";
                    if libc::write(fd, ready.as_ptr().cast(), ready.len()) != 1 {
                        libc::_exit(126);
                    }
                    let mut gate = libc::pollfd {
                        fd,
                        events: libc::POLLIN,
                        revents: 0,
                    };
                    if libc::poll(&mut gate, 1, 10_000) != 1 {
                        libc::_exit(124);
                    }
                    let mut release = [0u8];
                    if libc::read(fd, release.as_mut_ptr().cast(), release.len()) != 1
                        || release != *b"G"
                    {
                        libc::_exit(126);
                    }
                    Ok(())
                });
            }
            let spawned = command.spawn();
            drop(child_control);
            spawned
        });
        let mut fork = Self {
            control,
            launch: Some(launch),
        };
        let mut ready = [0];
        fork.control.read_exact(&mut ready)?;
        if ready != *b"R" {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(fork)
    }
    fn release(&mut self) -> io::Result<ExitStatus> {
        // Always unblock and join, including if writing the release itself fails.
        let sent = self.control.write_all(b"G");
        if sent.is_err() {
            let _ = self.control.shutdown(std::net::Shutdown::Both);
        }
        let launched = self
            .launch
            .take()
            .expect("one launch owner")
            .join()
            .map_err(|_| io::Error::other("concurrent launcher panicked"))?;
        let mut child = ChildGuard(launched?);
        let status = child.finish();
        sent?;
        status
    }
}
impl Drop for BlockedFork {
    fn drop(&mut self) {
        if self.launch.is_some() {
            let _ = self.release();
        }
    }
}
fn execute_image(image: &HelperImage) -> io::Result<ExitStatus> {
    // Invalid protocol arguments exit 125 immediately. That proves exec occurred
    // without launching a workload or relying on invented helper probe behavior.
    let mut child = ChildGuard(
        Command::new(image.path())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?,
    );
    child.finish()
}

#[test]
fn constructor_image_executes_while_unrelated_child_is_between_fork_and_exec() {
    let mut blocked = None;
    let constructed = {
        let mut staging_observer = || {
            assert!(blocked.is_none(), "materialization hook must run once");
            // This runs in the constructing parent while its real materializer is
            // active. It never opens the image, writable or otherwise.
            blocked = Some(BlockedFork::start().expect("concurrent fork handshake"));
        };
        HelperImage::materialize(None, Some(&mut staging_observer))
    };
    let mut blocked = blocked.expect("actual constructor handoff was observed");
    let during = constructed.as_ref().ok().map(execute_image);
    // Release and reap the unrelated process BEFORE inspecting/asserting any
    // constructor or execution result. A failing RED cannot strand the blocker.
    let released = blocked.release();
    drop(blocked);
    let image = constructed.expect("helper image construction");
    assert!(released.expect("blocker release/reap").success());
    assert_eq!(fs::read(image.path()).unwrap(), IMAGE);
    let after = execute_image(&image).expect("same image executes after inherited writer releases");
    assert_eq!(after.code(), Some(125));
    let during = during.expect("constructor succeeded");
    assert!(
        during.is_ok(),
        "constructed helper exec blocked by unrelated pre-exec child: kind={:?} errno={:?}",
        during.as_ref().err().map(io::Error::kind),
        during.as_ref().err().and_then(io::Error::raw_os_error)
    );
    assert_eq!(during.unwrap().code(), Some(125));
}
