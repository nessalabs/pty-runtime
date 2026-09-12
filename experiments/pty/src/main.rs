//! Reproducible measurement fixtures, deliberately separate from the future runtime.
mod handoff;
mod platform;

use serde_json::{json, Value};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    io::{self, Read, Write},
    mem::zeroed,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc, Condvar, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

struct Counting;
static LIVE: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = System.alloc(l);
        if !p.is_null() {
            LIVE.fetch_add(l.size(), Ordering::Relaxed);
        }
        p
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        let p = System.alloc_zeroed(l);
        if !p.is_null() {
            LIVE.fetch_add(l.size(), Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Ordering::Relaxed);
        System.dealloc(p, l);
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        let q = System.realloc(p, l, n);
        if !q.is_null() {
            if n >= l.size() {
                LIVE.fetch_add(n - l.size(), Ordering::Relaxed);
            } else {
                LIVE.fetch_sub(l.size() - n, Ordering::Relaxed);
            }
        }
        q
    }
}
#[global_allocator]
static ALLOC: Counting = Counting;

pub(crate) fn monotonic_ns() -> u64 {
    let mut t = unsafe { zeroed() };
    assert_eq!(
        unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut t) },
        0
    );
    t.tv_sec as u64 * 1_000_000_000 + t.tv_nsec as u64
}
pub(crate) fn sleep_until(target: u64) {
    let now = monotonic_ns();
    if target > now {
        thread::sleep(Duration::from_nanos(target - now));
    }
}
pub(crate) fn cpu_us(who: i32) -> u64 {
    let mut r: libc::rusage = unsafe { zeroed() };
    assert_eq!(unsafe { libc::getrusage(who, &mut r) }, 0);
    ((r.ru_utime.tv_sec + r.ru_stime.tv_sec) as u64) * 1_000_000
        + (r.ru_utime.tv_usec + r.ru_stime.tv_usec) as u64
}
pub(crate) fn sample() -> Value {
    // Capture live requested bytes before constructing the measurement JSON.
    let heap = LIVE.load(Ordering::Relaxed);
    let mut v = platform::memory();
    v["live_heap_bytes"] = json!(heap);
    v["cpu_us"] = json!(cpu_us(libc::RUSAGE_SELF));
    v["descriptors"] = json!(platform::descriptor_count());
    v
}
pub(crate) fn cleanup_sample(base: &Value) -> Value {
    let start = Instant::now();
    loop {
        let mut value = sample();
        if value["threads"] == base["threads"] || start.elapsed() >= Duration::from_secs(1) {
            value["settle_ms"] = json!(start.elapsed().as_secs_f64() * 1000.);
            return value;
        }
        thread::sleep(Duration::from_millis(10));
    }
}
pub(crate) fn clean_check(base: &Value, cleaned: &Value) {
    assert_eq!(
        base["descriptors"], cleaned["descriptors"],
        "descriptor cleanup"
    );
    assert_eq!(base["threads"], cleaned["threads"], "reader cleanup");
}
pub(crate) fn nonblock(fd: &OwnedFd) {
    let flags = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFL) };
    assert!(flags >= 0);
    assert_eq!(
        unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) },
        0
    );
}
pub(crate) fn pair() -> (OwnedFd, OwnedFd) {
    unsafe {
        let (mut host, mut child) = (-1, -1);
        assert_eq!(
            libc::openpty(
                &mut host,
                &mut child,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut()
            ),
            0,
            "openpty: {}",
            io::Error::last_os_error()
        );
        let mut attr = zeroed();
        assert_eq!(libc::tcgetattr(child, &mut attr), 0);
        libc::cfmakeraw(&mut attr);
        assert_eq!(libc::tcsetattr(child, libc::TCSANOW, &attr), 0);
        for fd in [host, child] {
            assert_eq!(libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC), 0);
        }
        (OwnedFd::from_raw_fd(host), OwnedFd::from_raw_fd(child))
    }
}
pub(crate) fn read_fd(fd: &OwnedFd, b: &mut [u8]) -> io::Result<usize> {
    let n = unsafe { libc::read(fd.as_raw_fd(), b.as_mut_ptr().cast(), b.len()) };
    if n < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(n as usize)
    }
}
pub(crate) fn write_all_fd(fd: i32, mut b: &[u8]) {
    while !b.is_empty() {
        let n = unsafe { libc::write(fd, b.as_ptr().cast(), b.len()) };
        if n < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            if e.kind() == io::ErrorKind::WouldBlock {
                let mut p = libc::pollfd {
                    fd,
                    events: libc::POLLOUT,
                    revents: 0,
                };
                assert!(
                    unsafe { libc::poll(&mut p, 1, 1000) } > 0,
                    "write readiness deadline"
                );
                continue;
            }
            panic!("write: {e}");
        }
        assert!(n > 0);
        b = &b[n as usize..];
    }
}

#[repr(align(64))]
#[derive(Default)]
struct Counter {
    bytes: AtomicU64,
    calls: AtomicU64,
    checksum: AtomicU64,
    lock: Mutex<()>,
    wake: Condvar,
}
impl Counter {
    fn add(&self, b: &[u8]) {
        let sum = b.iter().map(|x| u64::from(*x)).sum::<u64>();
        self.checksum.fetch_add(sum, Ordering::Relaxed);
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.bytes.fetch_add(b.len() as u64, Ordering::Release);
        let _guard = self.lock.lock().unwrap();
        self.wake.notify_all();
    }
    fn wait(&self, n: u64) {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut guard = self.lock.lock().unwrap();
        while self.bytes.load(Ordering::Acquire) < n {
            let now = Instant::now();
            assert!(now < deadline, "read deadline");
            guard = self.wake.wait_timeout(guard, deadline - now).unwrap().0;
        }
    }
    fn check(&self, n: u64, byte: u8) {
        self.wait(n);
        assert_eq!(self.bytes.load(Ordering::Acquire), n);
        assert_eq!(self.checksum.load(Ordering::Relaxed), n * u64::from(byte));
    }
}
struct Readers {
    children: Vec<OwnedFd>,
    counters: Arc<Vec<Counter>>,
    joins: Vec<JoinHandle<()>>,
    probe: Option<OwnedFd>,
}
impl Readers {
    fn start(model: &str, n: usize, read_bytes: usize, probe: bool) -> Self {
        assert!(n > 0 && n <= 1024 && (1..=65536).contains(&read_bytes));
        let mut hosts = Vec::with_capacity(n);
        let mut children = Vec::with_capacity(n);
        for _ in 0..n {
            let (h, c) = pair();
            hosts.push(h);
            children.push(c);
        }
        let probe = probe.then(|| hosts[n - 1].try_clone().unwrap());
        let counters = Arc::new((0..n).map(|_| Counter::default()).collect::<Vec<_>>());
        let ready = Arc::new(AtomicUsize::new(0));
        let mut joins = Vec::new();
        if model == "dedicated" {
            for (i, host) in hosts.into_iter().enumerate() {
                let counters = counters.clone();
                let ready = ready.clone();
                joins.push(
                    thread::Builder::new()
                        .name("pty-dedicated".into())
                        .stack_size(64 * 1024)
                        .spawn(move || {
                            let mut b = vec![0u8; read_bytes];
                            ready.fetch_add(1, Ordering::Release);
                            loop {
                                match read_fd(&host, &mut b) {
                                    Ok(0) => break,
                                    Ok(n) => counters[i].add(&b[..n]),
                                    Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                                    Err(e) if e.raw_os_error() == Some(libc::EIO) => break,
                                    Err(e) => panic!("read: {e}"),
                                }
                            }
                        })
                        .unwrap(),
                );
            }
        } else {
            let workers = reader_workers(model, n);
            for h in &hosts {
                nonblock(h);
            }
            let mut groups: Vec<Vec<_>> = (0..workers).map(|_| Vec::new()).collect();
            for (i, h) in hosts.into_iter().enumerate() {
                groups[i % workers].push((i, h));
            }
            for group in groups {
                let counters = counters.clone();
                let ready = ready.clone();
                joins.push(
                    thread::Builder::new()
                        .name("pty-shared".into())
                        .spawn(move || {
                            let (sessions, endpoints): (Vec<_>, Vec<_>) = group.into_iter().unzip();
                            let local_n = endpoints.len();
                            let mut reactor = platform::Readiness::new(&endpoints).unwrap();
                            let mut closed = vec![false; local_n];
                            let mut remaining = local_n;
                            let mut b = vec![0u8; read_bytes];
                            let mut indices = Vec::with_capacity(local_n);
                            ready.fetch_add(local_n, Ordering::Release);
                            while remaining > 0 {
                                indices.clear();
                                match reactor.wait() {
                                    Ok(v) => indices.extend_from_slice(v),
                                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                                    Err(e) => panic!("readiness: {e}"),
                                }
                                for &i in &indices {
                                    if closed[i] {
                                        continue;
                                    }
                                    let mut budget: usize = 64 * 1024;
                                    loop {
                                        match read_fd(&endpoints[i], &mut b) {
                                            Ok(0) => {
                                                closed[i] = true;
                                                break;
                                            }
                                            Ok(n) => {
                                                counters[sessions[i]].add(&b[..n]);
                                                budget = budget.saturating_sub(n);
                                                if budget == 0 {
                                                    break;
                                                }
                                            }
                                            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                                                break
                                            }
                                            Err(e) if e.kind() == io::ErrorKind::Interrupted => {
                                                continue
                                            }
                                            Err(e) if e.raw_os_error() == Some(libc::EIO) => {
                                                closed[i] = true;
                                                break;
                                            }
                                            Err(e) => panic!("read: {e}"),
                                        }
                                    }
                                    if closed[i] {
                                        remaining -= 1;
                                        reactor.remove(&endpoints[i]);
                                    }
                                }
                            }
                        })
                        .unwrap(),
                );
            }
        }
        let start = Instant::now();
        while ready.load(Ordering::Acquire) < n {
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "reader start deadline"
            );
            thread::yield_now();
        }
        Self {
            children,
            counters,
            joins,
            probe,
        }
    }
    fn stop(self) {
        drop(self.probe);
        drop(self.children);
        for join in self.joins {
            join.join().unwrap();
        }
    }
}

pub(crate) fn reader_workers(model: &str, n: usize) -> usize {
    if model == "dedicated" {
        return n;
    }
    let workers = if model == "shared" {
        1
    } else {
        model
            .strip_prefix("shared-")
            .unwrap()
            .parse::<usize>()
            .unwrap()
    };
    assert!((1..=16).contains(&workers));
    workers.min(n)
}

pub(crate) fn quantiles(v: &mut [f64]) -> Value {
    assert!(!v.is_empty());
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let q = |p: f64| v[((v.len() as f64 * p).ceil() as usize).saturating_sub(1)];
    json!({"samples":v.len(),"p50_us":q(0.5),"p95_us":q(0.95),"p99_us":q(0.99),"max_us":v[v.len()-1]})
}
fn idle(model: &str, n: usize, read_bytes: usize, millis: u64) -> Value {
    let base = sample();
    let r = Readers::start(model, n, read_bytes, false);
    for (i, e) in r.children.iter().enumerate() {
        write_all_fd(e.as_raw_fd(), b"x");
        r.counters[i].check(1, b'x');
    }
    thread::sleep(Duration::from_millis(200));
    let resident = sample();
    let before = cpu_us(libc::RUSAGE_SELF);
    let time = Instant::now();
    thread::sleep(Duration::from_millis(millis));
    let elapsed = time.elapsed().as_secs_f64();
    let cpu = cpu_us(libc::RUSAGE_SELF) - before;
    r.stop();
    let cleaned = cleanup_sample(&base);
    clean_check(&base, &cleaned);
    json!({"case":"idle","base":base,"resident":resident,"cleaned":cleaned,
        "idle_cpu_percent":cpu as f64/elapsed/10000.,"seconds":elapsed,"verified":true})
}
fn serial(model: &str, n: usize, read_bytes: usize, mib: usize) -> Value {
    let base = sample();
    let r = Readers::start(model, n, read_bytes, false);
    let b = [b'x'; 4096];
    let cpu0 = cpu_us(libc::RUSAGE_SELF);
    let now = Instant::now();
    for _ in 0..mib * 1024 * 1024 / b.len() {
        for e in &r.children {
            write_all_fd(e.as_raw_fd(), &b);
        }
    }
    let target = (mib * 1024 * 1024) as u64;
    for c in r.counters.iter() {
        c.check(target, b'x');
    }
    let elapsed = now.elapsed().as_secs_f64();
    let cpu = cpu_us(libc::RUSAGE_SELF) - cpu0;
    let resident = sample();
    let mut latency = Vec::with_capacity(2000);
    let mut counts = vec![target; n];
    for round in 0..2000 {
        let i = round % n;
        let now = Instant::now();
        write_all_fd(r.children[i].as_raw_fd(), b"x");
        counts[i] += 1;
        r.counters[i].wait(counts[i]);
        latency.push(now.elapsed().as_secs_f64() * 1e6);
    }
    for (c, &expected) in r.counters.iter().zip(&counts) {
        c.check(expected, b'x');
    }
    let latency = quantiles(&mut latency);
    drop(counts);
    r.stop();
    let cleaned = cleanup_sample(&base);
    clean_check(&base, &cleaned);
    json!({"case":"serial","bytes":target*n as u64,"seconds":elapsed,
        "aggregate_mib_per_sec":mib as f64*n as f64/elapsed,
        "owner_cpu_ms_per_mib":cpu as f64/1000./(mib*n)as f64,
        "sequential_handshake":latency,"base":base,"resident":resident,"cleaned":cleaned,"verified":true})
}

pub(crate) struct ManagedChild(pub(crate) Child);
impl Drop for ManagedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn producer(duration_ms: u64, rate: u64) {
    io::stderr().write_all(b"R").unwrap();
    let mut start = [0; 8];
    io::stdin().read_exact(&mut start).unwrap();
    let start = u64::from_le_bytes(start);
    sleep_until(start);
    let end = start + duration_ms * 1_000_000;
    let b = [b'x'; 4096];
    let mut total = 0u64;
    let mut write_ns = 0;
    let cpu0 = cpu_us(libc::RUSAGE_SELF);
    while monotonic_ns() < end {
        let before = monotonic_ns();
        write_all_fd(libc::STDOUT_FILENO, &b);
        write_ns += monotonic_ns() - before;
        total += b.len() as u64;
        if rate > 0 {
            sleep_until((start + (total as u128 * 1_000_000_000 / rate as u128) as u64).min(end));
        }
    }
    eprintln!(
        "{}",
        json!({"bytes":total,"cpu_us":cpu_us(libc::RUSAGE_SELF)-cpu0,
        "write_wall_ns":write_ns,"stop_ns":monotonic_ns()})
    );
}
fn echo() {
    io::stderr().write_all(b"R").unwrap();
    let mut b = [0u8; 1];
    while io::stdin().read_exact(&mut b).is_ok() {
        write_all_fd(libc::STDOUT_FILENO, &b);
    }
}
fn concurrent(
    model: &str,
    n: usize,
    read_bytes: usize,
    active: usize,
    duration_ms: u64,
    rate: u64,
) -> Value {
    assert!(active > 0 && active <= n && n < 1024 && duration_ms >= 200);
    let base = sample();
    let r = Readers::start(model, n + 1, read_bytes, true);
    let exe = std::env::current_exe().unwrap();
    let mut children = Vec::with_capacity(active);
    for endpoint in &r.children[..active] {
        let mut child = ManagedChild(
            Command::new(&exe)
                .arg("__produce")
                .arg(duration_ms.to_string())
                .arg(rate.to_string())
                .stdin(Stdio::piped())
                .stdout(Stdio::from(endpoint.try_clone().unwrap()))
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let mut ready = [0];
        child
            .0
            .stderr
            .as_mut()
            .unwrap()
            .read_exact(&mut ready)
            .unwrap();
        assert_eq!(ready, [b'R']);
        children.push(child);
    }
    let mut probe = ManagedChild(
        Command::new(&exe)
            .arg("__echo")
            .stdin(Stdio::from(r.children[n].try_clone().unwrap()))
            .stdout(Stdio::from(r.children[n].try_clone().unwrap()))
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut ready = [0];
    probe
        .0
        .stderr
        .as_mut()
        .unwrap()
        .read_exact(&mut ready)
        .unwrap();
    assert_eq!(ready, [b'R']);
    let start = monotonic_ns() + 200_000_000;
    let end = start + duration_ms * 1_000_000;
    for c in &mut children {
        c.0.stdin
            .take()
            .unwrap()
            .write_all(&start.to_le_bytes())
            .unwrap();
    }
    sleep_until(start);
    let cpu0 = cpu_us(libc::RUSAGE_SELF);
    let mut latency = Vec::new();
    let mut probes = 0;
    while monotonic_ns() < end {
        let before = monotonic_ns();
        write_all_fd(r.probe.as_ref().unwrap().as_raw_fd(), b"p");
        probes += 1;
        r.counters[n].wait(probes);
        latency.push((monotonic_ns() - before) as f64 / 1000.);
        sleep_until((before + 10_000_000).min(end));
    }
    let mut producer_data = Vec::with_capacity(active);
    let mut total = 0u64;
    for (i, c) in children.iter_mut().enumerate() {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = c.0.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(Instant::now() < deadline, "producer exit deadline");
            thread::sleep(Duration::from_millis(1));
        }
        let mut result = String::new();
        c.0.stderr
            .take()
            .unwrap()
            .read_to_string(&mut result)
            .unwrap();
        let data: Value = serde_json::from_str(&result).unwrap();
        let bytes = data["bytes"].as_u64().unwrap();
        assert!(bytes > 0);
        r.counters[i].check(bytes, b'x');
        total += bytes;
        producer_data.push(data);
    }
    for c in &r.counters[active..n] {
        c.check(0, b'x');
    }
    r.counters[n].check(probes, b'p');
    let elapsed = (monotonic_ns() - start) as f64 / 1e9;
    let cpu = cpu_us(libc::RUSAGE_SELF) - cpu0;
    let resident = sample();
    let latency = quantiles(&mut latency);
    drop(probe);
    drop(children);
    r.stop();
    let cleaned = cleanup_sample(&base);
    clean_check(&base, &cleaned);
    let min = producer_data
        .iter()
        .map(|x| x["bytes"].as_u64().unwrap())
        .min()
        .unwrap();
    let max = producer_data
        .iter()
        .map(|x| x["bytes"].as_u64().unwrap())
        .max()
        .unwrap();
    json!({"case":"concurrent","bytes":total,"seconds":elapsed,"active_producers":active,
        "probe_ptys":1,"offered_bytes_per_sec_per_producer":rate,
        "aggregate_mib_per_sec":total as f64/1048576./elapsed,
        "owner_cpu_ms_per_mib":cpu as f64/1000./(total as f64/1048576.),
        "under_load_roundtrip":latency,"producer_min_bytes":min,"producer_max_bytes":max,
        "producers":producer_data,"base":base,"resident":resident,"cleaned":cleaned,"verified":true})
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args[1] == "__produce" {
        producer(args[2].parse().unwrap(), args[3].parse().unwrap());
        return;
    }
    if args[1] == "__echo" {
        echo();
        return;
    }
    if args[1] == "__ramp" {
        handoff::ramp(args[2].parse().unwrap(), args[3].parse().unwrap());
        return;
    }
    let model = &args[2];
    let n = args[3].parse().unwrap();
    let read_bytes = args[4].parse().unwrap();
    let mut result = match args[1].as_str() {
        "idle" => idle(model, n, read_bytes, args[5].parse().unwrap()),
        "serial" => serial(model, n, read_bytes, args[5].parse().unwrap()),
        "concurrent" => concurrent(
            model,
            n,
            read_bytes,
            args[5].parse().unwrap(),
            args[6].parse().unwrap(),
            args[7].parse().unwrap(),
        ),
        "handoff" => handoff::handoff(
            model,
            n,
            read_bytes,
            args[5].parse().unwrap(),
            args[6].parse().unwrap(),
            args[7].parse().unwrap(),
            args[8].parse().unwrap(),
            args[9].parse().unwrap(),
            args[10].parse().unwrap(),
        ),
        _ => panic!("unknown experiment"),
    };
    result["model"] = json!(model);
    result["ptys"] = json!(n);
    result["read_buffer_bytes"] = json!(read_bytes);
    result["reader_workers"] = json!(reader_workers(
        model,
        n + usize::from(args[1] == "concurrent")
    ));
    result["readiness_backend"] = json!(platform::BACKEND);
    result["protocol"] = json!(2);
    println!("{result}");
}
