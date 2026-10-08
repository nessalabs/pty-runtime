//! The parent must still hold the child PTY endpoint after the workload has
//! exited, and the bytes that workload queued must then be delivered.
//!
//! Linux keeps those bytes even if the parent drops the endpoint, so a green
//! byte assertion on Linux does not prove the descriptor stayed open. The
//! open-descriptor check is what fails on Linux when the endpoint is dropped
//! at spawn. macOS discards unread bytes in `ttyclose` when the last reference
//! closes at session teardown, including a tail left after a short read.
//!
//! The check happens after the workload has exited and before the reader runs.
//! It does not wait for the session leader. macOS session exit can itself wait
//! for pending terminal output — the runtime drains before it waits for helper
//! exit for that reason — so a test that waits for the leader while the reader
//! is parked stalls on the bytes it is trying to keep. A 9000-byte payload did
//! that on macOS: the write also exceeded the terminal queue (historical
//! ceiling 1024 bytes, `TTYHOG`), so the workload never finished either.
//!
//! The payload is larger than one read and smaller than that ceiling. Closing
//! after the first read would drop the tail on macOS once the workload's own
//! descriptors are gone. Linux keeps that tail, so the byte assertion is not
//! the Linux lock.
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
    time::Duration,
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
fn retained_slave_is_open_after_the_workload_exits_and_the_queued_bytes_arrive() {
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let release_rx = Mutex::new(release_rx);
    let reader_hold = Arc::new(move || {
        let _ = entered_tx.send(());
        let _ = release_rx.lock().unwrap().recv();
    });
    let (noted_tx, noted_rx) = mpsc::channel();
    let note_sentinel = Arc::new(move |_sentinel, fd| {
        let _ = noted_tx.send((fd, descriptor_path(fd)));
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
    const QUEUED: usize = 64;
    let mut expected = vec![b'A'; QUEUED];
    expected.extend_from_slice(b"END");
    let limits = ProcessLimits {
        read_chunk: 16,
        ..ProcessLimits::default()
    };
    assert!(
        expected.len() > limits.read_chunk,
        "payload must outlast one read, or closing after that read has no tail to drop"
    );
    assert!(
        expected.len() < 1024,
        "payload must fit in the macOS terminal queue, or the write blocks while the reader is parked"
    );
    let spec = CommandSpec::new(
        "/bin/sh".into(),
        std::env::temp_dir(),
        vec![
            "-c".into(),
            format!("dd if=/dev/zero bs={QUEUED} count=1 2>/dev/null | tr '\\0' A; printf END")
                .into(),
        ],
    )
    .unwrap();
    owner
        .spawn(
            &spec,
            TerminalSize::new(80, 24).unwrap(),
            SessionLifetime::new(90, 1),
            limits,
            events.clone(),
        )
        .unwrap();
    entered_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("reader did not reach its hold");
    let (slave_fd, slave_path) = noted_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("launch did not report the child endpoint");
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
    // The workload has finished writing and closed its descriptors. The reader
    // is still parked, so this is before any read. The session leader is not
    // part of the check: its exit can wait on this unread output.
    events.wait(|state| state.exit.is_some());
    assert_eq!(
        descriptor_path(slave_fd).as_ref(),
        Some(&slave_path),
        "parent dropped {slave_path:?} (fd {slave_fd}) before reading. macOS \
         flushes unread PTY output when that last reference closes at \
         session-leader teardown"
    );
    let _ = release.0.take().unwrap().send(());
    events.wait(|state| state.drain.is_some());
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
