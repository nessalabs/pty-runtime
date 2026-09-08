use super::{
    DEADLINE, Result, config::Config, fixture::monotonic_ns, observe::Observer, payload,
    population::Population, resources, wire::Frame,
};
use pty_runtime::{AttachPosition, LatencyKind, RuntimeDiagnostics, RuntimeError, TerminalSize};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
pub struct PhaseResult {
    pub bytes: u64,
    pub seconds: f64,
    pub completion_seconds: f64,
    pub fixture_rtt: std::sync::Arc<RuntimeDiagnostics>,
}
fn controls(
    population: &mut Population,
    pending: &mut BTreeMap<u64, Instant>,
    rtt: &RuntimeDiagnostics,
) -> Result<()> {
    for child in &mut population.children {
        for _ in 0..64 {
            let Some(frame) = child.reader.next(&mut child.socket)? else {
                break;
            };
            match frame.kind {
                b'i' => {
                    let started = pending
                        .remove(&frame.values[0])
                        .ok_or_else(|| std::io::Error::other("unknown input acknowledgement"))?;
                    rtt.record(LatencyKind::InputDispatch, started.elapsed(), true);
                }
                b'd' => {
                    child.phase_done = true;
                    child.total = frame.values[1];
                    child.replies = frame.values[2];
                    child.summary = frame.values;
                    assert_eq!(frame.values[6], 0, "fixture input/query errors");
                }
                b'e' => {
                    child.window = [frame.values[3], frame.values[4], frame.values[2]];
                    println!(
                        "{{\"event\":\"producer_end\",\"producer\":{},\"phase\":{},\"elapsed_ns\":{},\"phase_bytes\":{},\"start_monotonic_ns\":{},\"end_monotonic_ns\":{}}}",
                        child.producer,
                        frame.values[0],
                        frame.values[1],
                        frame.values[2],
                        frame.values[3],
                        frame.values[4]
                    );
                }
                b'w' => {
                    child.writes = frame.values;
                    println!(
                        "{{\"event\":\"producer_writes\",\"producer\":{},\"phase\":{},\"write_syscall_ns\":{},\"max_write_ns\":{},\"eagain_count\":{},\"partial_write_count\":{},\"byte_cap\":{},\"cap_exhausted\":{}}}",
                        child.producer,
                        frame.values[0],
                        frame.values[1],
                        frame.values[2],
                        frame.values[3],
                        frame.values[4],
                        frame.values[5],
                        frame.values[6] != 0
                    );
                }
                b'p' => {
                    child.total = frame.values[0];
                }
                b's' => println!(
                    "{{\"event\":\"producer_start\",\"producer\":{},\"phase\":{},\"unix_ns\":{}}}",
                    child.producer, frame.values[0], frame.values[1]
                ),
                _ => return Err(std::io::Error::other("unexpected fixture response").into()),
            }
        }
    }
    Ok(())
}
pub async fn run(
    population: &mut Population,
    observers: &mut [Vec<Observer>],
    config: &Config,
    phase: u64,
    seconds: u64,
) -> Result<PhaseResult> {
    let before_bytes: u64 = population.children.iter().map(|child| child.total).sum();
    let start = monotonic_ns() + 100_000_000;
    for child in &mut population.children {
        child.phase_done = false;
        child.window = [0; 3];
        child.writes = [0; 7];
        Frame::new(
            b'b',
            &[
                phase,
                seconds,
                config.rate_for(child.producer),
                config.chunk as u64,
                start,
                u64::from(config.mode == "saturation" && child.producer < config.active),
                config.producer_bytes,
            ],
        )
        .write(&mut child.socket)?;
    }
    let began = Instant::now();
    let deadline = began + Duration::from_secs(seconds) + DEADLINE;
    let rtt = RuntimeDiagnostics::new();
    let mut pending = BTreeMap::new();
    let mut cancel = None::<super::cancel::Probe>;
    let mut probe = 0u64;
    let mut last_input = Instant::now();
    let mut last_resize = Instant::now();
    let mut last_cancel = Instant::now();
    let mut last_report = Instant::now();
    while population.children.iter().any(|child| !child.phase_done) || cancel.is_some() {
        controls(population, &mut pending, &rtt)?;
        let running = population.children.iter().any(|child| !child.phase_done);
        if let Some(probe) = &mut cancel {
            if probe.poll(&population.runtime)? {
                cancel = None;
            }
        }
        if matches!(
            config.mode.as_str(),
            "attached" | "dominant" | "stalled-sink" | "idle" | "saturation"
        ) {
            for session in observers.iter_mut() {
                for observer in session {
                    observer.poll()?;
                }
            }
        }
        if running
            && config.active > 0
            && last_input.elapsed() >= Duration::from_millis(50)
            && pending.len() < 128
        {
            probe += 1;
            let index = probe as usize % config.active;
            let mut bytes = [b'P'; 9];
            bytes[1..].copy_from_slice(&probe.to_le_bytes());
            pending.insert(probe, Instant::now());
            let written =
                tokio::time::timeout(DEADLINE, population.children[index].session.write(&bytes)?)
                    .await?;
            assert!(written.error.is_none(), "{written:?}");
            assert_eq!(written.written, bytes.len());
            last_input = Instant::now();
        }
        if running && config.active > 0 && last_resize.elapsed() >= Duration::from_millis(100) {
            let child = &population.children[probe as usize % config.active];
            let size = TerminalSize::new(config.cols, config.rows).unwrap();
            if config.raw {
                tokio::time::timeout(DEADLINE, child.session.resize(size)?).await??;
            } else {
                let result = tokio::time::timeout(DEADLINE, child.session.resize_projected(size)?)
                    .await?
                    .map_err(RuntimeError::from)?;
                assert!(result.os.is_ok() && result.model.is_ok(), "{result:?}");
            }
            last_resize = Instant::now();
        }
        if running
            && config.active > 0
            && cancel.is_none()
            && last_cancel.elapsed() >= Duration::from_secs(1)
        {
            let child = population.spawn(config, config.sessions, true)?;
            cancel = Some(super::cancel::Probe::start(child)?);
            last_cancel = Instant::now();
        }
        if last_report.elapsed() >= Duration::from_secs(1) {
            resources::report("under_load", &population.runtime.resources(), false);
            last_report = Instant::now();
        }
        assert!(Instant::now() < deadline, "producer phase timed out");
        assert!(
            pending.values().all(|started| started.elapsed() < DEADLINE),
            "input RTT timed out"
        );
        tokio::time::sleep(Duration::from_millis(if config.active == 0 {
            100
        } else {
            1
        }))
        .await;
    }
    let completion_seconds = began.elapsed().as_secs_f64();
    if population.children.iter().any(|child| child.writes[6] != 0) {
        return Err(std::io::Error::other(
            "producer byte cap exhausted; capacity trial is censored",
        )
        .into());
    }
    let measured = &population.children[..config.active.max(1)];
    let elapsed = measurement_seconds(measured.iter().map(|child| child.window))?;
    // Producers have stopped, but PTY reads, parsing, replies and admitted input can still be in flight.
    let settle = Instant::now();
    loop {
        for child in &mut population.children {
            Frame::new(b'q', &[]).write(&mut child.socket)?;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
        controls(population, &mut pending, &rtt)?;
        let mut ready = pending.is_empty();
        for child in &population.children {
            let tail = child.session.attach(AttachPosition::Tail)?.cursor().offset;
            ready &= tail == child.total;
            if let Some(status) = child.session.projection_status()? {
                assert!(status.failure.is_none(), "{status:?}");
                ready &= status.processed.offset == child.total;
                ready &= child.replies == payload::queries(child.total);
            }
        }
        if ready {
            break;
        }
        if settle.elapsed() >= DEADLINE {
            for child in &population.children {
                eprintln!(
                    "unsettled producer={} total={} replies={} expected={} projection={:?}",
                    child.producer,
                    child.total,
                    child.replies,
                    payload::queries(child.total),
                    child.session.projection_status()?
                );
            }
            panic!("raw/projection/query accounting failed to settle");
        }
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    controls(population, &mut pending, &rtt)?;
    assert!(pending.is_empty());
    for child in &population.children {
        println!(
            "{{\"event\":\"producer_done\",\"phase\":{phase},\"producer\":{},\"bytes_total\":{},\"query_replies\":{},\"write_blocked_ns\":{},\"max_backpressure_wait_ns\":{},\"write_calls\":{},\"phase_bytes\":{},\"producer_bytes_per_second\":{}}}",
            child.producer,
            child.total,
            child.replies,
            child.summary[3],
            child.summary[4],
            child.summary[5],
            child.window[2],
            child.window[2] as f64 * 1_000_000_000.0 / (child.window[1] - child.window[0]) as f64
        );
    }
    let accepted = population
        .children
        .iter()
        .map(|child| child.total)
        .sum::<u64>()
        - before_bytes;
    assert_eq!(
        accepted,
        population
            .children
            .iter()
            .map(|child| child.window[2])
            .sum::<u64>(),
        "producer window byte accounting mismatch"
    );
    Ok(PhaseResult {
        bytes: accepted,
        seconds: elapsed,
        completion_seconds,
        fixture_rtt: rtt,
    })
}

fn measurement_seconds(windows: impl Iterator<Item = [u64; 3]>) -> Result<f64> {
    let mut first = u64::MAX;
    let mut last = 0;
    for [start, end, _] in windows {
        if start == 0 || end <= start {
            return Err(
                std::io::Error::other("missing or invalid producer measurement window").into(),
            );
        }
        first = first.min(start);
        last = last.max(end);
    }
    if last <= first {
        return Err(std::io::Error::other("empty measurement window").into());
    }
    Ok((last - first) as f64 / 1_000_000_000.0)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_window_includes_producer_start_skew() {
        assert_eq!(
            measurement_seconds(
                [
                    [1_000_000_000, 3_000_000_000, 10],
                    [2_000_000_000, 4_000_000_000, 20]
                ]
                .into_iter()
            )
            .unwrap(),
            3.0
        );
        assert!(measurement_seconds([[0, 0, 0]].into_iter()).is_err());
    }
}
