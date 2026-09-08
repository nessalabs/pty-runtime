use super::{Result, payload, wire::Frame};
use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
pub fn unix_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
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
                let [phase, seconds, rate, chunk, start, ..] = frame.values;
                if chunk == 0 || chunk > 65536 {
                    return Err(std::io::Error::other("invalid producer chunk").into());
                }
                std::thread::sleep(Duration::from_nanos(start.saturating_sub(unix_ns())));
                let began = Instant::now();
                emit(&output, Frame::new(b's', &[phase, unix_ns()]))?;
                let mut bytes = vec![0; chunk as usize];
                let mut stream = std::io::stdout().lock();
                let mut phase_bytes = 0u64;
                let mut writes = 0;
                let mut blocked = 0;
                let mut maximum = 0;
                let mut progress = Instant::now();
                while began.elapsed() < Duration::from_secs(seconds) {
                    if rate == 0 {
                        std::thread::sleep(Duration::from_millis(10));
                        continue;
                    }
                    let target = Duration::from_secs_f64(phase_bytes as f64 / rate as f64);
                    if let Some(wait) = target.checked_sub(began.elapsed()) {
                        std::thread::sleep(wait);
                    }
                    if began.elapsed() >= Duration::from_secs(seconds) {
                        break;
                    }
                    payload::fill(&mut bytes, offset, producer);
                    let before = Instant::now();
                    stream.write_all(&bytes)?;
                    stream.flush()?;
                    let elapsed = before.elapsed().as_nanos() as u64;
                    blocked += elapsed;
                    maximum = maximum.max(elapsed);
                    writes += 1;
                    offset += bytes.len() as u64;
                    phase_bytes += bytes.len() as u64;
                    if progress.elapsed() >= Duration::from_secs(1) {
                        emit(&output, Frame::new(b'p', &[offset]))?;
                        progress = Instant::now();
                    }
                }
                emit(
                    &output,
                    Frame::new(
                        b'e',
                        &[phase, began.elapsed().as_nanos() as u64, phase_bytes],
                    ),
                )?;
                summary = [
                    phase,
                    offset,
                    replies.load(Ordering::Relaxed),
                    blocked,
                    maximum,
                    writes,
                    errors.load(Ordering::Relaxed),
                ];
                emit(&output, Frame::new(b'd', &summary))?;
            }
            _ => return Err(std::io::Error::other("unknown fixture control command").into()),
        }
    }
}
