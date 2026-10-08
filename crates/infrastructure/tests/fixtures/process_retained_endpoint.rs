//! The parent must still hold the child PTY endpoint after the session leader
//! has exited, and the bytes written before that exit must then be delivered.
//!
//! Linux keeps those bytes even if the parent drops the endpoint, so a green
//! byte assertion on Linux does not prove the descriptor stayed open. The
//! open-descriptor check is what fails on Linux when the endpoint is dropped
//! at spawn. macOS discards unread bytes in `ttyclose` when the leader's
//! teardown closes the last reference, including a tail left after a short
//! read. The payload is larger than one read so that tail is part of the
//! assertion. Both checks fail there if this parent drops the endpoint early.
#![cfg(test)]
use super::{backend::UnixProcessBackend, process_test_support::*, spawner};
use pty_runtime_application::process::IProcessBackend;
use pty_runtime_domain::{
    SessionLifetime,
    process::{CommandSpec, DrainOutcome, ExitStatus, ProcessLimits},
    terminal::TerminalSize,
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};
struct Release(Option<mpsc::Sender<()>>);
impl Drop for Release {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.send(());
        }
    }
}

#[test]
fn retained_slave_keeps_output_written_before_the_session_leader_exits() {
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let release_rx = Mutex::new(release_rx);
    let reader_hold = Arc::new(move || {
        let _ = entered_tx.send(());
        let _ = release_rx.lock().unwrap().recv();
    });
    let (noted_tx, noted_rx) = mpsc::channel();
    let note_sentinel = Arc::new(move |sentinel, fd| {
        let _ = noted_tx.send((sentinel, fd, descriptor_path(fd)));
    });
    let owner = Arc::new(
        UnixProcessBackend::build(
            vec![std::env::temp_dir()],
            2,
            spawner::Options {
                bundled: None,
                hook: None,
                after_launch: None,
                note_sentinel: Some(note_sentinel),
                reader_hold: Some(reader_hold),
            },
        )
        .unwrap(),
    );
    // Dropping the owner joins the reader. The release guard is declared
    // second so a panic unblocks that reader before the join.
    let mut release = Release(Some(release_tx));
    let events = Arc::new(Events::default());
    // 9000 bytes is more than one 4096-byte read. The marker is the tail.
    // Closing the child endpoint after the first read drops that tail on
    // macOS. Linux keeps it, so this assertion is not the Linux lock; the
    // descriptor check below is.
    let mut expected = vec![b'A'; 9000];
    expected.extend_from_slice(b"END");
    let spec = CommandSpec::new(
        "/bin/sh".into(),
        std::env::temp_dir(),
        vec![
            "-c".into(),
            "dd if=/dev/zero bs=9000 count=1 2>/dev/null | tr '\\0' A; printf END".into(),
        ],
    )
    .unwrap();
    owner
        .spawn(
            &spec,
            TerminalSize::new(80, 24).unwrap(),
            SessionLifetime::new(90, 1),
            ProcessLimits::default(),
            events.clone(),
        )
        .unwrap();
    entered_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("reader did not reach its hold");
    let (sentinel, slave_fd, slave_path) = noted_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("launch did not report the session leader");
    let slave_path = slave_path.expect("child endpoint path was not visible at launch");
    let shown = slave_path.to_string_lossy();
    assert!(
        shown.starts_with("/dev/") && (shown.contains("pts") || shown.contains("tty")),
        "launch reported {slave_path:?}, which is not the child endpoint"
    );
    assert!(
        !shown.ends_with("ptmx"),
        "launch reported the host endpoint {slave_path:?}"
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while !session_leader_finished(sentinel) {
        assert!(
            Instant::now() < deadline,
            "session leader {sentinel} was still alive 5s after the workload \
             exited, with its output unread; exiting is blocked on the terminal \
             buffer, so this parent cannot hold the endpoint across teardown"
        );
        // The runtime is not delaying. This test is waiting until the leader
        // has actually exited before it looks at the descriptor.
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        descriptor_path(slave_fd).as_ref(),
        Some(&slave_path),
        "parent dropped {slave_path:?} (fd {slave_fd}) before reading. macOS \
         flushes unread PTY output when that last reference closes at \
         session-leader teardown"
    );
    // The leader is already gone. Unblock the reader and require the bytes
    // that were queued before the leader exited.
    let _ = release.0.take().unwrap().send(());
    events.wait(|state| state.exit.is_some() && state.drain.is_some());
    let state = events.state.lock().unwrap();
    assert_eq!(state.bytes, expected);
    assert_eq!(state.exit, Some(ExitStatus::Code(0)));
    assert_eq!(state.drain, Some(DrainOutcome::Eof));
    drop(state);
    owner.shutdown();
}

fn descriptor_path(fd: i32) -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_link(format!("/proc/self/fd/{fd}")).ok()
    }
    #[cfg(target_os = "macos")]
    {
        let mut buf = vec![0u8; 1024];
        // SAFETY: F_GETPATH writes a NUL-terminated path into this buffer and
        // does not retain it. `fd` is the live child endpoint just opened.
        let rc = unsafe { libc::fcntl(fd, libc::F_GETPATH, buf.as_mut_ptr()) };
        if rc == -1 {
            return None;
        }
        let end = buf.iter().position(|byte| *byte == 0).unwrap_or(buf.len());
        let text = String::from_utf8_lossy(&buf[..end]);
        Some(PathBuf::from(text.as_ref()))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = fd;
        None
    }
}

fn session_leader_finished(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        leader_finished_linux(pid)
    }
    #[cfg(target_os = "macos")]
    {
        leader_finished_macos(pid)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = pid;
        false
    }
}

#[cfg(target_os = "linux")]
fn leader_finished_linux(pid: u32) -> bool {
    // Do not waitpid: the supervisor owns reaping this child. A missing
    // record means it has already been collected; Z means it has exited.
    let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return true;
    };
    let Some((_, rest)) = stat.rsplit_once(')') else {
        return false;
    };
    rest.split_whitespace().next() == Some("Z")
}

#[cfg(target_os = "macos")]
fn leader_finished_macos(pid: u32) -> bool {
    // SAFETY: proc_bsdinfo is a plain output record. Zero is initialized
    // storage, and proc_pidinfo writes at most the length passed in.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as i32;
    let count = unsafe {
        libc::proc_pidinfo(
            pid as libc::pid_t,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            size,
        )
    };
    if count != size {
        return true;
    }
    info.pbi_status == libc::SZOMB
}
