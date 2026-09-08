//! Scoped acceptance measurements, not the full G4 repetition or soak contract.
#![cfg(feature = "ghostty")]
#[allow(dead_code)]
mod support;
use pty_runtime::*;
use std::{
    future::Future,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
    time::{Duration, Instant},
};
use support::{command, id};
const PAYLOAD: usize = 4 * 1024 * 1024;
struct Unpark(std::thread::Thread);
impl Wake for Unpark {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}
fn wait<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        if let Poll::Ready(result) = future.as_mut().poll(&mut context) {
            return result;
        }
        assert!(
            Instant::now() < deadline,
            "performance fixture deadline exceeded"
        );
        std::thread::park_timeout(deadline.saturating_duration_since(Instant::now()));
    }
}
fn rss() -> String {
    std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
        .and_then(|kb| kb.checked_mul(1024))
        .map_or("null".into(), |v| v.to_string())
}
fn descriptors() -> String {
    let path = if cfg!(target_os = "linux") {
        "/proc/self/fd"
    } else {
        "/dev/fd"
    };
    std::fs::read_dir(path)
        .ok()
        .map_or("null".into(), |entries| entries.count().to_string())
}
fn descendants() -> String {
    let child = std::process::Command::new("ps")
        .args(["-axo", "pid=,ppid="])
        .stdout(std::process::Stdio::piped())
        .spawn();
    let Ok(child) = child else {
        return "null".into();
    };
    let sampler = child.id();
    let Ok(output) = child.wait_with_output() else {
        return "null".into();
    };
    if !output.status.success() {
        return "null".into();
    }
    let Ok(text) = String::from_utf8(output.stdout) else {
        return "null".into();
    };
    let rows = text
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            Some((
                fields.next()?.parse::<u32>().ok()?,
                fields.next()?.parse::<u32>().ok()?,
            ))
        })
        .collect::<Vec<_>>();
    let mut owned = std::collections::HashSet::from([std::process::id()]);
    loop {
        let before = owned.len();
        for &(pid, parent) in &rows {
            if pid != sampler && owned.contains(&parent) {
                owned.insert(pid);
            }
        }
        if before == owned.len() {
            return (owned.len() - 1).to_string();
        }
    }
}
fn bytes(attachment: &mut Attachment) -> Vec<u8> {
    match wait(attachment.read_next()).unwrap() {
        OutputEvent::Replay(ReplayPage::Bytes { bytes, .. }) => bytes,
        other => panic!("lossless performance observer received {other:?}"),
    }
}
fn run(count: usize) {
    let began = Instant::now();
    let limits = RuntimeOptions {
        replay_bytes: count * (PAYLOAD + 4096),
        ..RuntimeOptions::default()
    };
    let runtime = Runtime::new(vec![std::env::current_dir().unwrap()], limits.clone()).unwrap();
    let mut sessions = Vec::new();
    let mut attachments = Vec::new();
    for index in 0..count {
        let config = terminal::TerminalConfig::new(TerminalSize::new(80, 24).unwrap());
        let mut options = SessionOptions::projected(config);
        options.replay_bytes = PAYLOAD + 4096;
        options.projection.as_mut().unwrap().park_after = Duration::from_secs(300);
        let session = runtime
            .spawn(
                id(&format!("performance-{index}")),
                &command(
                    "performance-producer",
                    &[&PAYLOAD.to_string(), &index.to_string()],
                ),
                options,
            )
            .unwrap();
        let mut attachment = session.attach(AttachPosition::Oldest).unwrap();
        let mut ready = Vec::new();
        while ready.len() < 5 {
            ready.extend(bytes(&mut attachment));
        }
        assert_eq!(ready, b"READY");
        sessions.push(session);
        attachments.push(attachment);
    }
    let startup = began.elapsed();
    let ready_rss = rss();
    let ready_fds = descriptors();
    let ready_descendants = descendants();
    let released = Instant::now();
    let (release_send, release_receive) = std::sync::mpsc::channel();
    let mut consumers = Vec::new();
    for (index, mut attachment) in attachments.into_iter().enumerate() {
        consumers.push(std::thread::spawn(move || {
            let suffix = format!("\x1b[2J\x1b[HPERF-END-{index}").into_bytes();
            let expected = PAYLOAD + suffix.len();
            let mut offset = 0;
            while offset < expected {
                for byte in bytes(&mut attachment) {
                    let wanted = if offset >= PAYLOAD {
                        suffix[offset - PAYLOAD]
                    } else if offset % 80 == 79 {
                        b'\n'
                    } else {
                        b'A' + ((offset + index) % 26) as u8
                    };
                    assert_eq!(byte, wanted, "raw mismatch session {index} offset {offset}");
                    offset += 1;
                    assert!(offset <= expected);
                }
            }
            offset + 5
        }));
    }
    std::thread::scope(|scope| {
        for session in &sessions {
            let sender = release_send.clone();
            scope.spawn(move || {
                let outcome = wait(session.write(b"G").unwrap());
                assert_eq!(outcome.written, 1);
                assert_eq!(outcome.error, None);
                sender.send(released.elapsed().as_nanos()).unwrap();
            });
        }
    });
    drop(release_send);
    let releases = release_receive.into_iter().collect::<Vec<_>>();
    let skew = releases.iter().max().unwrap() - releases.iter().min().unwrap();
    let expected = consumers
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect::<Vec<_>>();
    for (session, expected) in sessions.iter().zip(&expected) {
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let status = session.projection_status().unwrap().unwrap();
            assert_eq!(status.failure, None);
            assert_eq!(status.parking_failure, None);
            assert_eq!(status.published.offset as usize, *expected);
            if status.processed.offset as usize == *expected {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "parser did not reach exact producer count"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    let elapsed = released.elapsed();
    let final_rss = rss();
    for (index, session) in sessions.iter().enumerate() {
        let view = wait(session.projected_view().unwrap()).unwrap();
        let row = view.view().cells[..80]
            .iter()
            .map(|cell| cell.text.as_str())
            .collect::<String>();
        assert_eq!(row.trim_end(), format!("PERF-END-{index}"));
        let sent = wait(session.write(b"X").unwrap());
        assert_eq!(sent.written, 1);
        assert_eq!(sent.error, None);
        let done = wait(session.wait().unwrap()).unwrap();
        assert_eq!(done.status.exit, Some(ExitStatus::Code(0)));
        assert_eq!(done.status.drain, Some(DrainOutcome::Eof));
    }
    let cleanup_start = Instant::now();
    runtime.shutdown();
    for session in &sessions {
        assert_eq!(
            session.projection_status().unwrap().unwrap().residency,
            Residency::Closed
        );
    }
    let cleanup = cleanup_start.elapsed();
    let closed_fds = descriptors();
    let closed_descendants = descendants();
    let processed: usize = expected.iter().sum();
    println!(
        "PERFORMANCE_JSON {{\"schema\":1,\"os\":\"{}\",\"arch\":\"{}\",\"build\":\"{}\",\"sessions\":{},\"payload_bytes_per_session\":{},\"raw_bytes_validated\":{},\"parser_processed_bytes\":{},\"startup_ns\":{},\"release_completion_skew_ns\":{},\"elapsed_ns\":{},\"payload_bytes_per_second\":{},\"cleanup_ns\":{},\"rss_ready_bytes\":{},\"rss_after_processing_bytes\":{},\"open_fds_ready\":{},\"open_fds_after_shutdown\":{},\"descendant_processes_ready\":{},\"descendant_processes_after_shutdown\":{},\"peak_rss_bytes\":null,\"pss_bytes\":null,\"resident_budget_bytes\":{},\"staging_budget_bytes\":{},\"replay_budget_bytes\":{},\"success\":true}}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        count,
        PAYLOAD,
        processed,
        processed,
        startup.as_nanos(),
        skew,
        elapsed.as_nanos(),
        (count * PAYLOAD) as f64 / elapsed.as_secs_f64(),
        cleanup.as_nanos(),
        ready_rss,
        final_rss,
        ready_fds,
        closed_fds,
        ready_descendants,
        closed_descendants,
        limits.projection.resident_bytes,
        limits.projection.staging_bytes,
        limits.replay_bytes
    );
}
#[test]
#[ignore = "scoped release performance measurement: scripts/performance.py"]
fn projected_real_pty_producers() {
    for count in [1, 16] {
        run(count);
    }
}
