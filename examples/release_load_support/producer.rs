//! Bounded nonblocking producer. Only POLLOUT waits are classified as backpressure.
use super::payload;
use std::{
    io,
    os::fd::RawFd,
    time::{Duration, Instant},
};
pub struct Plan {
    pub duration: Duration,
    pub rate: u64,
    pub unpaced: bool,
    pub chunk: usize,
    pub cap: u64,
    pub producer: usize,
}
#[derive(Default, Debug)]
pub struct Outcome {
    pub bytes: u64,
    pub wait_ns: u64,
    pub max_wait_ns: u64,
    pub syscall_ns: u64,
    pub max_syscall_ns: u64,
    pub attempts: u64,
    pub eagain: u64,
    pub partial: u64,
    pub capped: bool,
}
pub fn run(fd: RawFd, offset: u64, plan: Plan) -> io::Result<Outcome> {
    let began = Instant::now();
    let deadline = began + plan.duration;
    let mut result = Outcome::default();
    let mut bytes = vec![0; plan.chunk];
    while Instant::now() < deadline {
        if result.bytes == plan.cap {
            break;
        }
        if !plan.unpaced {
            let wait = if plan.rate == 0 {
                Duration::from_millis(10)
            } else {
                Duration::from_secs_f64(result.bytes as f64 / plan.rate as f64)
                    .saturating_sub(began.elapsed())
            };
            std::thread::sleep(wait.min(deadline.saturating_duration_since(Instant::now())));
            if plan.rate == 0 || Instant::now() >= deadline {
                continue;
            }
        }
        let length = (plan.cap - result.bytes).min(plan.chunk as u64) as usize;
        payload::fill(&mut bytes[..length], offset + result.bytes, plan.producer);
        let mut sent = 0;
        while sent < length && Instant::now() < deadline {
            let before = Instant::now();
            // SAFETY: fd remains open for this call; this initialized byte slice is
            // valid for length-sent bytes, and write does not retain its pointer.
            // The caller provides a separate nonblocking output file description.
            let n = unsafe { libc::write(fd, bytes[sent..length].as_ptr().cast(), length - sent) };
            let elapsed = before.elapsed().as_nanos() as u64;
            result.syscall_ns += elapsed;
            result.max_syscall_ns = result.max_syscall_ns.max(elapsed);
            result.attempts += 1;
            if n > 0 {
                if (n as usize) < length - sent {
                    result.partial += 1;
                }
                sent += n as usize;
                result.bytes += n as u64;
                continue;
            }
            if n == 0 {
                return Err(io::ErrorKind::WriteZero.into());
            }
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            if error.kind() != io::ErrorKind::WouldBlock {
                return Err(error);
            }
            result.eagain += 1;
            let remaining = deadline.saturating_duration_since(Instant::now());
            let timeout = remaining.as_millis().min(i32::MAX as u128) as i32;
            let mut poll = libc::pollfd {
                fd,
                events: libc::POLLOUT,
                revents: 0,
            };
            let before = Instant::now();
            // SAFETY: poll points to one initialized pollfd for the duration of
            // the synchronous call; the borrowed output descriptor remains open.
            let polled = unsafe { libc::poll(&mut poll, 1, timeout) };
            let elapsed = before.elapsed().as_nanos() as u64;
            result.wait_ns += elapsed;
            result.max_wait_ns = result.max_wait_ns.max(elapsed);
            if polled < 0 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
                return Err(io::Error::last_os_error());
            }
            if poll.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                return Err(io::Error::other(format!(
                    "producer output poll failed: revents={}",
                    poll.revents
                )));
            }
        }
    }
    // The deadline can end the loop immediately after the final successful write.
    // Cap exhaustion is a property of the final ledger, regardless of exit path.
    result.capped = result.bytes == plan.cap;
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::Read,
        os::{fd::AsRawFd, unix::net::UnixStream},
    };
    fn plan(unpaced: bool, cap: u64) -> Plan {
        Plan {
            duration: Duration::from_millis(50),
            rate: 0,
            unpaced,
            chunk: 65536,
            cap,
            producer: 2,
        }
    }
    #[test]
    fn stalled_output_has_bounded_deadline_and_exact_partial_ledger() {
        let (output, mut input) = UnixStream::pair().unwrap();
        output.set_nonblocking(true).unwrap();
        let before = Instant::now();
        let result = run(output.as_raw_fd(), 0, plan(true, 1024 * 1024 * 1024)).unwrap();
        assert!(before.elapsed() < Duration::from_secs(2));
        assert!(result.eagain > 0 && result.wait_ns > 0 && result.bytes > 0);
        assert!(!result.capped);
        drop(output);
        let mut received = Vec::new();
        input.read_to_end(&mut received).unwrap();
        assert_eq!(result.bytes, received.len() as u64);
        let mut expected = vec![0; received.len()];
        payload::fill(&mut expected, 0, 2);
        assert_eq!(received, expected);
    }
    #[test]
    fn zero_rate_idle_is_distinct_from_unpaced_and_cap_is_explicit() {
        let (output, _input) = UnixStream::pair().unwrap();
        output.set_nonblocking(true).unwrap();
        assert_eq!(
            run(output.as_raw_fd(), 0, plan(false, 100)).unwrap().bytes,
            0
        );
        let result = run(output.as_raw_fd(), 0, plan(true, 100)).unwrap();
        assert_eq!(result.bytes, 100);
        assert!(result.capped);
    }
}
