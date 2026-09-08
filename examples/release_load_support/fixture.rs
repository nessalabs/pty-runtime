use super::{Result, producer, wire::Frame};
use std::{
    io::Read,
    os::{
        fd::AsRawFd,
        unix::{fs::OpenOptionsExt, net::UnixStream},
    },
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
pub fn unix_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}
pub fn monotonic_ns() -> u64 {
    let mut now = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: now is writable and lives through this synchronous call.
    assert_eq!(
        unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut now) },
        0
    );
    now.tv_sec as u64 * 1_000_000_000 + now.tv_nsec as u64
}
fn emit(socket: &Mutex<UnixStream>, frame: Frame) -> std::io::Result<()> {
    frame.write(&mut socket.lock().unwrap())
}
pub fn run(path: &str, producer: usize) -> Result<()> {
    if !std::process::Command::new("/bin/stty")
        .args(["raw", "-echo"])
        .status()?
        .success()
    {
        return Err(std::io::Error::other("raw fixture PTY setup failed").into());
    }
    let mut command = UnixStream::connect(path)?;
    let output = Arc::new(Mutex::new(command.try_clone()?));
    let replies = Arc::new(AtomicU64::new(0));
    let errors = Arc::new(AtomicU64::new(0));
    let (writer, counts, invalid) = (output.clone(), replies.clone(), errors.clone());
    std::thread::spawn(move || {
        let mut input = std::io::stdin().lock();
        let mut first = [0];
        while input.read_exact(&mut first).is_ok() {
            let result = if first[0] == b'P' {
                let mut id = [0; 8];
                input
                    .read_exact(&mut id)
                    .and_then(|()| emit(&writer, Frame::new(b'i', &[u64::from_le_bytes(id)])))
            } else if first[0] == 27 {
                let mut tail = [0; 5];
                input.read_exact(&mut tail).map(|()| {
                    if tail == *b"[1;1R" {
                        counts.fetch_add(1, Ordering::Relaxed);
                    } else {
                        invalid.fetch_add(1, Ordering::Relaxed);
                    }
                })
            } else {
                invalid.fetch_add(1, Ordering::Relaxed);
                Ok(())
            };
            if result.is_err() {
                break;
            }
        }
    });
    let mut tty = [0i8; 1024];
    // SAFETY: tty is writable for its full supplied length; stdout remains open.
    let status = unsafe { libc::ttyname_r(libc::STDOUT_FILENO, tty.as_mut_ptr(), tty.len()) };
    if status != 0 {
        return Err(std::io::Error::from_raw_os_error(status).into());
    }
    // SAFETY: successful ttyname_r guarantees NUL termination inside tty.
    let tty = unsafe { std::ffi::CStr::from_ptr(tty.as_ptr()) }.to_str()?;
    let stream = std::fs::OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY)
        .open(tty)?;
    emit(&output, Frame::new(b'r', &[std::process::id().into()]))?;
    let mut offset = 0u64;
    let mut summary = [0; 7];
    loop {
        let frame = Frame::read(&mut command)?;
        match frame.kind {
            b'x' => return Ok(()),
            b'q' => {
                summary[2] = replies.load(Ordering::Relaxed);
                summary[6] = errors.load(Ordering::Relaxed);
                emit(&output, Frame::new(b'd', &summary))?;
            }
            b'b' => {
                let [phase, seconds, rate, chunk, start, unpaced, cap] = frame.values;
                if chunk == 0 || chunk > 65536 || cap == 0 {
                    return Err(std::io::Error::other("invalid producer bounds").into());
                }
                std::thread::sleep(Duration::from_nanos(start.saturating_sub(monotonic_ns())));
                emit(
                    &output,
                    Frame::new(b's', &[phase, unix_ns(), monotonic_ns()]),
                )?;
                let began = monotonic_ns();
                let result = producer::run(
                    stream.as_raw_fd(),
                    offset,
                    producer::Plan {
                        duration: Duration::from_secs(seconds),
                        rate,
                        unpaced: unpaced != 0,
                        chunk: chunk as usize,
                        cap,
                        producer,
                    },
                )?;
                let ended = monotonic_ns();
                offset += result.bytes;
                emit(
                    &output,
                    Frame::new(b'e', &[phase, ended - began, result.bytes, began, ended]),
                )?;
                emit(
                    &output,
                    Frame::new(
                        b'w',
                        &[
                            phase,
                            result.syscall_ns,
                            result.max_syscall_ns,
                            result.eagain,
                            result.partial,
                            cap,
                            u64::from(result.capped),
                        ],
                    ),
                )?;
                summary = [
                    phase,
                    offset,
                    replies.load(Ordering::Relaxed),
                    result.wait_ns,
                    result.max_wait_ns,
                    result.attempts,
                    errors.load(Ordering::Relaxed),
                ];
                emit(&output, Frame::new(b'd', &summary))?;
            }
            _ => return Err(std::io::Error::other("unknown fixture control command").into()),
        }
    }
}
