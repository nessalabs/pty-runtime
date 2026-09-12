//! Reader-placement handoff measurement. This is a standalone fixture.
//!
//! Nothing here is a library feature: the production runtime keeps one dedicated
//! reader per live PTY for the whole life of the session. This fixture exists to
//! measure what a bounded dynamic placement policy *would* cost before anyone
//! decides whether to build one, as required by ADR 0003.
//!
//! One descriptor has exactly one owner at a time. Ownership moves only after the
//! previous owner has stopped and published every byte it already read, and each
//! new owner checks for pending data immediately. A per-session ownership flag
//! rejects readiness events that name a descriptor the worker no longer owns.

use crate::{
    clean_check, cleanup_sample, cpu_us, monotonic_ns, nonblock, pair, platform, quantiles,
    read_fd, sample, sleep_until, write_all_fd, ManagedChild,
};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    io::{self, Read, Write},
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc, Condvar, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

/// Default disposition is "ignore", so a stray delivery cannot kill the fixture,
/// and the handler is installed without `SA_RESTART` so a blocked `read` returns
/// `EINTR` instead of resuming.
const INTERRUPT: libc::c_int = libc::SIGURG;
/// Readiness index reserved for a worker's own control pipe.
const CONTROL: usize = usize::MAX;
/// Per readiness event, matching the fixed-placement shared readers.
const FAIRNESS_BUDGET: usize = 64 * 1024;
const RAMP: usize = 256;

extern "C" fn on_interrupt(_: libc::c_int) {}

fn install_interrupt() {
    // SAFETY: the handler is empty, touches no process state, and cannot unwind.
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = on_interrupt as *const () as usize;
        libc::sigemptyset(&mut action.sa_mask);
        action.sa_flags = 0;
        assert_eq!(
            libc::sigaction(INTERRUPT, &action, std::ptr::null_mut()),
            0,
            "install interrupt handler: {}",
            io::Error::last_os_error()
        );
    }
}

fn blocking(fd: &OwnedFd) {
    let flags = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFL) };
    assert!(flags >= 0);
    assert_eq!(
        unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, flags & !libc::O_NONBLOCK) },
        0
    );
}

/// Per-session delivery state. Exactly one reader owns it at a time, so the
/// running offset is read and written without a lock; each handoff publishes it
/// through the acknowledgement that transfers ownership.
#[derive(Default)]
struct Delivery {
    bytes: AtomicU64,
    checksum: AtomicU64,
    disordered: AtomicU64,
}

impl Delivery {
    /// Producers emit a continuous 256-byte ramp, so the expected value of every
    /// absolute offset is known. This detects loss, duplication and reordering
    /// across a handoff, at the cost of one comparison per byte.
    fn accept(&self, chunk: &[u8]) {
        let start = self.bytes.load(Ordering::Relaxed);
        let mut sum = 0u64;
        let mut wrong = 0u64;
        for (index, value) in chunk.iter().enumerate() {
            sum += u64::from(*value);
            if *value != ((start as usize + index) % RAMP) as u8 {
                wrong += 1;
            }
        }
        self.checksum.fetch_add(sum, Ordering::Relaxed);
        if wrong > 0 {
            self.disordered.fetch_add(wrong, Ordering::Relaxed);
        }
        self.bytes
            .store(start + chunk.len() as u64, Ordering::Release);
    }
    fn delivered(&self) -> u64 {
        self.bytes.load(Ordering::Acquire)
    }
}

struct Fabric {
    hosts: Vec<OwnedFd>,
    delivery: Vec<Delivery>,
    read_bytes: usize,
}

impl Fabric {
    /// Read everything currently available from one non-blocking endpoint.
    /// Returns the bytes drained so an adopting owner can report that it found
    /// data waiting for it.
    fn drain(&self, index: usize, buffer: &mut [u8]) -> u64 {
        let mut budget = FAIRNESS_BUDGET;
        let mut drained = 0;
        loop {
            match read_fd(&self.hosts[index], buffer) {
                Ok(0) => return drained,
                Ok(n) => {
                    self.delivery[index].accept(&buffer[..n]);
                    drained += n as u64;
                    budget = budget.saturating_sub(n);
                    if budget == 0 {
                        return drained;
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => return drained,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) if e.raw_os_error() == Some(libc::EIO) => return drained,
                Err(e) => panic!("drain: {e}"),
            }
        }
    }
}

enum Transfer {
    Adopt(usize),
    Release(usize),
    Shutdown,
}

/// One bounded shared readiness worker and its control channel.
struct Worker {
    control: OwnedFd,
    pending: Mutex<VecDeque<Transfer>>,
    acknowledged: Mutex<u64>,
    woken: Condvar,
    issued: AtomicU64,
    adopted_bytes: AtomicU64,
    stale_events: AtomicU64,
}

impl Worker {
    fn start(fabric: Arc<Fabric>, capacity: usize) -> (Arc<Self>, JoinHandle<()>) {
        let (read_end, write_end) = unsafe {
            let mut fds = [-1; 2];
            assert_eq!(libc::pipe(fds.as_mut_ptr()), 0, "control pipe");
            for fd in fds {
                assert_eq!(libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC), 0);
            }
            (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1]))
        };
        nonblock(&read_end);
        let worker = Arc::new(Self {
            control: write_end,
            pending: Mutex::new(VecDeque::new()),
            acknowledged: Mutex::new(0),
            woken: Condvar::new(),
            issued: AtomicU64::new(0),
            adopted_bytes: AtomicU64::new(0),
            stale_events: AtomicU64::new(0),
        });
        let handle = {
            let worker = worker.clone();
            thread::Builder::new()
                .name("pty-handoff-shared".into())
                .spawn(move || worker.serve(fabric, read_end, capacity))
                .expect("shared worker")
        };
        (worker, handle)
    }

    fn serve(&self, fabric: Arc<Fabric>, control: OwnedFd, capacity: usize) {
        let mut reactor = platform::Readiness::with_capacity(capacity + 1).expect("readiness");
        reactor
            .add(&control, CONTROL)
            .expect("control registration");
        let mut owned = vec![false; fabric.hosts.len()];
        let mut buffer = vec![0u8; fabric.read_bytes];
        let mut signal = [0u8; 64];
        let mut indices = Vec::new();
        loop {
            indices.clear();
            match reactor.wait() {
                Ok(ready) => indices.extend_from_slice(ready),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => panic!("readiness: {e}"),
            }
            let mut stop = false;
            for &index in &indices {
                if index != CONTROL {
                    // An ownership flag, not the readiness registration alone,
                    // decides whether this worker may touch the descriptor.
                    if owned[index] {
                        fabric.drain(index, &mut buffer);
                    } else {
                        self.stale_events.fetch_add(1, Ordering::Relaxed);
                    }
                    continue;
                }
                while read_fd(&control, &mut signal).is_ok_and(|n| n > 0) {}
                loop {
                    let Some(command) = self.pending.lock().expect("pending").pop_front() else {
                        break;
                    };
                    match command {
                        Transfer::Adopt(session) => {
                            reactor
                                .add(&fabric.hosts[session], session)
                                .expect("adopt registration");
                            owned[session] = true;
                            let found = fabric.drain(session, &mut buffer);
                            self.adopted_bytes.fetch_add(found, Ordering::Relaxed);
                        }
                        Transfer::Release(session) => {
                            fabric.drain(session, &mut buffer);
                            reactor.remove(&fabric.hosts[session]);
                            owned[session] = false;
                        }
                        Transfer::Shutdown => stop = true,
                    }
                    let mut acknowledged = self.acknowledged.lock().expect("acknowledged");
                    *acknowledged += 1;
                    self.woken.notify_all();
                }
            }
            if stop {
                assert!(
                    !owned.iter().any(|held| *held),
                    "worker still owns sessions"
                );
                return;
            }
        }
    }

    /// Submit one command and wait for the worker to finish it.
    fn request(&self, command: Transfer) {
        let expected = self.issued.fetch_add(1, Ordering::Relaxed) + 1;
        self.pending.lock().expect("pending").push_back(command);
        write_all_fd(self.control.as_raw_fd(), b"c");
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut acknowledged = self.acknowledged.lock().expect("acknowledged");
        while *acknowledged < expected {
            let now = Instant::now();
            assert!(now < deadline, "shared worker acknowledgement deadline");
            acknowledged = self
                .woken
                .wait_timeout(acknowledged, deadline - now)
                .expect("acknowledged")
                .0;
        }
    }
}

/// State shared with one dedicated reader thread while it owns a descriptor.
#[derive(Default)]
struct Owner {
    stop: AtomicBool,
    stopped: AtomicBool,
    thread: AtomicUsize,
}

struct Dedicated {
    owner: Arc<Owner>,
    join: JoinHandle<()>,
}

fn start_dedicated(fabric: &Arc<Fabric>, session: usize) -> Dedicated {
    let owner = Arc::new(Owner::default());
    let join = {
        let fabric = fabric.clone();
        let owner = owner.clone();
        thread::Builder::new()
            .name("pty-handoff-dedicated".into())
            .stack_size(64 * 1024)
            .spawn(move || {
                owner
                    .thread
                    .store(unsafe { libc::pthread_self() } as usize, Ordering::Release);
                let mut buffer = vec![0u8; fabric.read_bytes];
                loop {
                    match read_fd(&fabric.hosts[session], &mut buffer) {
                        Ok(0) => break,
                        Ok(n) => fabric.delivery[session].accept(&buffer[..n]),
                        Err(e) if e.kind() == io::ErrorKind::Interrupted => {
                            if owner.stop.load(Ordering::Acquire) {
                                break;
                            }
                        }
                        Err(e) if e.raw_os_error() == Some(libc::EIO) => break,
                        Err(e) => panic!("dedicated read: {e}"),
                    }
                }
                // Every byte this reader accepted is published before ownership
                // can move, because `accept` releases before this store.
                owner.stopped.store(true, Ordering::Release);
            })
            .expect("dedicated reader")
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while owner.thread.load(Ordering::Acquire) == 0 {
        assert!(Instant::now() < deadline, "dedicated reader start deadline");
        thread::yield_now();
    }
    Dedicated { owner, join }
}

/// Interrupt a silently blocked reader and wait for it to acknowledge. Retrying
/// covers the window between the reader's stop check and its next `read` call,
/// which is where a single signal would be lost.
fn stop_dedicated(reader: Dedicated, interrupts: &AtomicU64, retries: &AtomicU64) {
    reader.owner.stop.store(true, Ordering::Release);
    let thread = reader.owner.thread.load(Ordering::Acquire) as libc::pthread_t;
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut attempts = 0u64;
    let mut spins = 0u32;
    while !reader.owner.stopped.load(Ordering::Acquire) {
        if spins.is_multiple_of(64) {
            // SAFETY: the thread is joinable and not yet joined, so its handle
            // stays valid; an already-returned thread simply reports ESRCH.
            unsafe { libc::pthread_kill(thread, INTERRUPT) };
            attempts += 1;
        }
        spins += 1;
        if spins > 4096 {
            thread::sleep(Duration::from_micros(50));
        } else {
            thread::yield_now();
        }
        assert!(Instant::now() < deadline, "reader interrupt deadline");
    }
    interrupts.fetch_add(attempts, Ordering::Relaxed);
    retries.fetch_add(attempts.saturating_sub(1), Ordering::Relaxed);
    reader.join.join().expect("dedicated reader join");
}

struct Placement {
    fabric: Arc<Fabric>,
    workers: Vec<Arc<Worker>>,
    worker_joins: Vec<JoinHandle<()>>,
    readers: Vec<Option<Dedicated>>,
    interrupts: AtomicU64,
    retries: AtomicU64,
    spawns: AtomicU64,
}

impl Placement {
    fn worker(&self, session: usize) -> &Arc<Worker> {
        &self.workers[session % self.workers.len()]
    }
    /// Dedicated reader to shared worker.
    fn park(&mut self, session: usize) -> f64 {
        let started = monotonic_ns();
        let reader = self.readers[session].take().expect("dedicated owner");
        stop_dedicated(reader, &self.interrupts, &self.retries);
        nonblock(&self.fabric.hosts[session]);
        self.worker(session).request(Transfer::Adopt(session));
        (monotonic_ns() - started) as f64 / 1000.
    }
    /// Shared worker back to a dedicated reader.
    fn wake(&mut self, session: usize) -> f64 {
        let started = monotonic_ns();
        self.worker(session).request(Transfer::Release(session));
        blocking(&self.fabric.hosts[session]);
        self.readers[session] = Some(start_dedicated(&self.fabric, session));
        self.spawns.fetch_add(1, Ordering::Relaxed);
        (monotonic_ns() - started) as f64 / 1000.
    }
}

fn percentiles(values: &mut [f64]) -> Value {
    if values.is_empty() {
        return Value::Null;
    }
    quantiles(values)
}

/// A producer child writing the continuous ramp at a bounded rate.
pub fn ramp(duration_ms: u64, rate: u64) {
    assert!(rate > 0, "the handoff fixture throttles every producer");
    io::stderr().write_all(b"R").unwrap();
    let mut start = [0; 8];
    io::stdin().read_exact(&mut start).unwrap();
    let start = u64::from_le_bytes(start);
    sleep_until(start);
    let end = start + duration_ms * 1_000_000;
    let mut chunk = [0u8; 4096];
    for (index, value) in chunk.iter_mut().enumerate() {
        *value = (index % RAMP) as u8;
    }
    let mut total = 0u64;
    while monotonic_ns() < end {
        write_all_fd(libc::STDOUT_FILENO, &chunk);
        total += chunk.len() as u64;
        sleep_until((start + (total as u128 * 1_000_000_000 / rate as u128) as u64).min(end));
    }
    eprintln!("{}", json!({"bytes": total, "stop_ns": monotonic_ns()}));
}

struct Window {
    cpu_percent: f64,
    bytes: u64,
    seconds: f64,
}

fn observe(fabric: &Fabric, millis: u64) -> Window {
    let before_cpu = cpu_us(libc::RUSAGE_SELF);
    let before_bytes: u64 = fabric.delivery.iter().map(Delivery::delivered).sum();
    let clock = Instant::now();
    thread::sleep(Duration::from_millis(millis));
    let seconds = clock.elapsed().as_secs_f64();
    let cpu = cpu_us(libc::RUSAGE_SELF) - before_cpu;
    let after_bytes: u64 = fabric.delivery.iter().map(Delivery::delivered).sum();
    Window {
        cpu_percent: cpu as f64 / seconds / 10_000.,
        bytes: after_bytes - before_bytes,
        seconds,
    }
}

/// Round trip one byte through the probe session's echo child while the
/// population sits in its current placement. The wait spins rather than using a
/// condition variable so that no notification cost is added to every delivered
/// chunk; the probe therefore runs in its own window, outside the CPU window.
fn probe(fabric: &Fabric, session: usize, millis: u64, interval_us: u64) -> Vec<f64> {
    let mut samples = Vec::new();
    let mut sent = fabric.delivery[session].delivered();
    let end = monotonic_ns() + millis * 1_000_000;
    while monotonic_ns() < end {
        let issued = monotonic_ns();
        write_all_fd(
            fabric.hosts[session].as_raw_fd(),
            &[(sent % RAMP as u64) as u8],
        );
        sent += 1;
        let deadline = issued + 10_000_000_000;
        let mut spins = 0u32;
        while fabric.delivery[session].delivered() < sent {
            spins += 1;
            if spins.is_multiple_of(4096) {
                thread::yield_now();
                assert!(monotonic_ns() < deadline, "probe round trip deadline");
            } else {
                std::hint::spin_loop();
            }
        }
        samples.push((monotonic_ns() - issued) as f64 / 1000.);
        sleep_until((issued + interval_us * 1000).min(end));
    }
    samples
}

fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
    sorted[sorted.len() / 2]
}

#[allow(clippy::too_many_arguments)]
pub fn handoff(
    model: &str,
    n: usize,
    read_bytes: usize,
    active: usize,
    duration_ms: u64,
    rate: u64,
    cycles: usize,
    window_ms: u64,
    probe_ms: u64,
) -> Value {
    assert!(n > 0 && n <= 511 && active <= n && (1..=64).contains(&cycles));
    assert!(window_ms >= 100 && probe_ms >= 100 && (active == 0 || rate > 0));
    assert!(
        model != "dedicated",
        "handoff needs a shared placement target"
    );
    let workers = crate::reader_workers(model, n);
    install_interrupt();
    let base = sample();

    // Session `n` is the echo probe. It is registered, migrated and accounted
    // exactly like the others, so its round trip reports the latency of whichever
    // placement the population is currently in.
    let total_sessions = n + 1;
    let mut hosts = Vec::with_capacity(total_sessions);
    let mut children = Vec::with_capacity(total_sessions);
    for _ in 0..total_sessions {
        let (host, child) = pair();
        hosts.push(host);
        children.push(child);
    }
    let fabric = Arc::new(Fabric {
        hosts,
        delivery: (0..total_sessions).map(|_| Delivery::default()).collect(),
        read_bytes,
    });
    let (started, worker_joins): (Vec<_>, Vec<_>) = (0..workers)
        .map(|_| Worker::start(fabric.clone(), total_sessions))
        .unzip();
    let mut placement = Placement {
        workers: started,
        worker_joins,
        readers: (0..total_sessions)
            .map(|s| Some(start_dedicated(&fabric, s)))
            .collect(),
        fabric: fabric.clone(),
        interrupts: AtomicU64::new(0),
        retries: AtomicU64::new(0),
        spawns: AtomicU64::new(0),
    };

    let executable = std::env::current_exe().expect("fixture path");
    let mut producers = Vec::with_capacity(active);
    for endpoint in &children[..active] {
        let mut child = ManagedChild(
            Command::new(&executable)
                .arg("__ramp")
                .arg(duration_ms.to_string())
                .arg(rate.to_string())
                .stdin(Stdio::piped())
                .stdout(Stdio::from(
                    endpoint.try_clone().expect("producer endpoint"),
                ))
                .stderr(Stdio::piped())
                .spawn()
                .expect("producer"),
        );
        let mut ready = [0];
        child
            .0
            .stderr
            .as_mut()
            .expect("producer stderr")
            .read_exact(&mut ready)
            .expect("producer readiness");
        assert_eq!(ready, [b'R']);
        producers.push(child);
    }
    let mut echo = ManagedChild(
        Command::new(&executable)
            .arg("__echo")
            .stdin(Stdio::from(
                children[n].try_clone().expect("probe endpoint"),
            ))
            .stdout(Stdio::from(
                children[n].try_clone().expect("probe endpoint"),
            ))
            .stderr(Stdio::piped())
            .spawn()
            .expect("probe child"),
    );
    let mut ready = [0];
    echo.0
        .stderr
        .as_mut()
        .expect("probe stderr")
        .read_exact(&mut ready)
        .expect("probe readiness");
    assert_eq!(ready, [b'R']);
    let start = monotonic_ns() + 200_000_000;
    let end = start + duration_ms * 1_000_000;
    for child in &mut producers {
        child
            .0
            .stdin
            .take()
            .expect("producer start channel")
            .write_all(&start.to_le_bytes())
            .expect("producer start");
    }
    sleep_until(start);

    let mut park = (Vec::new(), Vec::new());
    let mut wake = (Vec::new(), Vec::new());
    let (mut dedicated_cpu, mut shared_cpu) = (Vec::new(), Vec::new());
    let (mut dedicated_rate, mut shared_rate) = (Vec::new(), Vec::new());
    let (mut dedicated_probe, mut shared_probe) = (Vec::new(), Vec::new());
    let mut peak_threads = base["threads"].as_u64().expect("thread count");
    let mut dedicated_memory = Value::Null;
    let mut shared_memory = Value::Null;
    for cycle in 0..cycles {
        let window = observe(&fabric, window_ms);
        dedicated_cpu.push(window.cpu_percent);
        dedicated_rate.push(window.bytes as f64 / 1048576. / window.seconds);
        if cycle == 0 {
            dedicated_memory = sample();
            peak_threads =
                peak_threads.max(dedicated_memory["threads"].as_u64().expect("thread count"));
        }
        dedicated_probe.extend(probe(&fabric, n, probe_ms, 5_000));
        for session in 0..total_sessions {
            let taken = placement.park(session);
            if session == n {
                continue;
            }
            if session < active {
                &mut park.0
            } else {
                &mut park.1
            }
            .push(taken);
        }
        let window = observe(&fabric, window_ms);
        shared_cpu.push(window.cpu_percent);
        shared_rate.push(window.bytes as f64 / 1048576. / window.seconds);
        if cycle == 0 {
            shared_memory = sample();
        }
        shared_probe.extend(probe(&fabric, n, probe_ms, 5_000));
        for session in 0..total_sessions {
            let taken = placement.wake(session);
            if session == n {
                continue;
            }
            if session < active {
                &mut wake.0
            } else {
                &mut wake.1
            }
            .push(taken);
        }
        assert!(
            active == 0 || monotonic_ns() < end,
            "producers finished before the placement cycles did; raise duration_ms"
        );
    }
    let probes = fabric.delivery[n].delivered();
    assert_eq!(
        probes as usize,
        dedicated_probe.len() + shared_probe.len(),
        "probe round trips and delivered probe bytes disagree"
    );

    let mut produced = Vec::with_capacity(active);
    let mut total = 0u64;
    for child in &mut producers {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(status) = child.0.try_wait().expect("producer status") {
                assert!(status.success());
                break;
            }
            assert!(Instant::now() < deadline, "producer exit deadline");
            thread::sleep(Duration::from_millis(1));
        }
        let mut text = String::new();
        child
            .0
            .stderr
            .take()
            .expect("producer stderr")
            .read_to_string(&mut text)
            .expect("producer result");
        let data: Value = serde_json::from_str(&text).expect("producer json");
        total += data["bytes"].as_u64().expect("producer bytes");
        produced.push(data);
    }
    for (session, expected) in produced.iter().enumerate() {
        let expected = expected["bytes"].as_u64().expect("producer bytes");
        assert!(expected > 0, "a producer made no progress");
        let deadline = Instant::now() + Duration::from_secs(30);
        while fabric.delivery[session].delivered() < expected {
            assert!(Instant::now() < deadline, "final delivery deadline");
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            fabric.delivery[session].delivered(),
            expected,
            "session {session} delivered a different byte count than its producer wrote"
        );
    }
    for session in active..n {
        assert_eq!(
            fabric.delivery[session].delivered(),
            0,
            "quiet session read"
        );
    }
    let disordered: u64 = fabric
        .delivery
        .iter()
        .map(|d| d.disordered.load(Ordering::Relaxed))
        .sum();
    // The probe session carries its own short ramp, so it is checksummed apart
    // from the producer traffic rather than folded into the workload total.
    let checksum: u64 = fabric.delivery[..n]
        .iter()
        .map(|d| d.checksum.load(Ordering::Relaxed))
        .sum();
    assert_eq!(disordered, 0, "bytes were lost, duplicated or reordered");
    assert_eq!(
        checksum,
        total / RAMP as u64 * (RAMP as u64 * (RAMP as u64 - 1) / 2),
        "ramp checksum"
    );

    let final_memory = sample();
    peak_threads = peak_threads.max(final_memory["threads"].as_u64().expect("thread count"));
    let interrupts = placement.interrupts.load(Ordering::Relaxed);
    let retries = placement.retries.load(Ordering::Relaxed);
    let spawns = placement.spawns.load(Ordering::Relaxed);
    let adopted: u64 = placement
        .workers
        .iter()
        .map(|w| w.adopted_bytes.load(Ordering::Relaxed))
        .sum();
    let stale: u64 = placement
        .workers
        .iter()
        .map(|w| w.stale_events.load(Ordering::Relaxed))
        .sum();

    for session in 0..total_sessions {
        let reader = placement.readers[session].take().expect("dedicated owner");
        stop_dedicated(reader, &placement.interrupts, &placement.retries);
    }
    for worker in &placement.workers {
        worker.request(Transfer::Shutdown);
    }
    for handle in placement.worker_joins.drain(..) {
        handle.join().expect("worker join");
    }
    drop(echo);
    drop(producers);
    drop(children);
    drop(placement);
    drop(fabric);
    let cleaned = cleanup_sample(&base);
    clean_check(&base, &cleaned);

    json!({"case":"handoff","ptys":n,"active_producers":active,"cycles":cycles,
        "shared_workers":workers,"window_ms":window_ms,"probe_ms":probe_ms,"probe_ptys":1,
        "offered_bytes_per_sec_per_producer":rate,
        "base":base,"dedicated":dedicated_memory,"resident":shared_memory,"cleaned":cleaned,
        "peak_threads":peak_threads,"bytes":total,"producers":produced,
        "dedicated_mib_per_sec":median(&dedicated_rate),
        "shared_mib_per_sec":median(&shared_rate),
        "dedicated_cpu_percent":median(&dedicated_cpu),
        "shared_cpu_percent":median(&shared_cpu),
        "dedicated_cpu_percent_runs":dedicated_cpu,"shared_cpu_percent_runs":shared_cpu,
        "dedicated_roundtrip":percentiles(&mut dedicated_probe),
        "shared_roundtrip":percentiles(&mut shared_probe),
        "to_shared_active_us":percentiles(&mut park.0),
        "to_shared_quiet_us":percentiles(&mut park.1),
        "to_dedicated_active_us":percentiles(&mut wake.0),
        "to_dedicated_quiet_us":percentiles(&mut wake.1),
        "migrations_per_direction":(cycles * total_sessions) as u64,
        "session_migrations_per_direction":(cycles * n) as u64,
        "reader_threads_created":spawns,"probe_roundtrips":probes,
        "interrupt_signals":interrupts,"interrupt_retries":retries,
        "adopted_pending_bytes":adopted,"stale_readiness_events":stale,
        "disordered_bytes":disordered,"checksum":checksum,"verified":true})
}
