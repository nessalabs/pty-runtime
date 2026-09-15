use super::{
    DEADLINE, Result, config::Config, fixture::monotonic_ns, observe::Observer, payload,
    population::Population, resize, resources, wire::Frame,
};
use pty_runtime::{AttachPosition, LatencyKind, RuntimeDiagnostics, TerminalSize};
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
/// The median of an ascending slice, averaging the middle pair when the count is
/// even.
///
/// Taking `slice[len / 2]` is the *upper* middle, which is the median only for
/// odd counts. The fairness workload runs 16 active producers, so every
/// distribution this reports was even-sized: `[1, 2, 100, 101]` was reported as
/// 100 rather than 51, distorting both ratios computed from it.
fn median_of(ascending: &[u64]) -> f64 {
    if ascending.is_empty() {
        return 0.0;
    }
    let middle = ascending.len() / 2;
    if ascending.len() % 2 == 1 {
        ascending[middle] as f64
    } else {
        // Averaged as f64 rather than as u64 so two adjacent values cannot sum
        // past the integer range, and so an odd total is not silently floored.
        (ascending[middle - 1] as f64 + ascending[middle] as f64) / 2.0
    }
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
        // ADR 0004 asks what happens after *all* observers detach while
        // producers keep running. `detached` never attaches any, which is a
        // different question: it never exercises the transition, and the
        // transition is where retention, parsing and admission have to notice
        // that nobody is reading any more. Half way through the measurement
        // phase, drop every attachment and keep producing.
        if config.mode == "detaching"
            && phase == 1
            && !observers.iter().all(Vec::is_empty)
            && began.elapsed() >= Duration::from_secs_f64(seconds as f64 / 2.0)
        {
            let released: usize = observers.iter().map(Vec::len).sum();
            for session in observers.iter_mut() {
                session.clear();
            }
            println!(
                "{{\"event\":\"observers_detached\",\"released\":{released},\"elapsed_seconds\":{:.3}}}",
                began.elapsed().as_secs_f64()
            );
            resources::report("after_detach", &population.runtime.resources(), false);
        }
        if matches!(
            config.mode.as_str(),
            "attached"
                | "detaching"
                | "reference"
                | "dominant"
                | "stalled-sink"
                | "idle"
                | "saturation"
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
                let result = resize::run(resize::Context {
                    runtime: &population.runtime,
                    session: &child.session,
                    producer: child.producer,
                    phase,
                    probe,
                    size,
                    began,
                })
                .await?;
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
    // ADR 0001's fairness clause is that "a flooding session or slow snapshot
    // consumer does not starve input, cancellation, resize, or other sessions",
    // and ADR 0004 asks for that as an *explicit outcome* rather than an
    // inference from aggregate throughput. Aggregate throughput cannot show it:
    // one session taking everything and one taking nothing sum to exactly the
    // same total as an even split.
    //
    // No threshold is invented here. The distribution is reported so a reader
    // can apply ADR 0002's targets, and the one property asserted is the one
    // that needs no threshold to name: a session that was asked to produce, in
    // a phase that delivered bytes, must not have been served none of them.
    let active: Vec<(usize, u64)> = population
        .children
        .iter()
        .filter(|child| child.producer < config.active)
        .map(|child| (child.producer, child.window[2]))
        .collect();
    let mut served: Vec<u64> = active.iter().map(|(_, bytes)| *bytes).collect();
    served.sort_unstable();
    if !served.is_empty() {
        let total: u64 = served.iter().sum();
        let median = median_of(&served);
        let lowest = served[0];
        let highest = served[served.len() - 1];
        // The question this ratio answers is what the *non-dominant* producers
        // got, so the dominant one is excluded by identity. `rate_for` gives
        // producer 0 nine tenths of the offered rate in `dominant` mode and
        // paces every producer alike in every other mode, so that index is who
        // the flooder is and there is nobody to exclude elsewhere.
        //
        // Dropping the numerically largest instead was wrong exactly where this
        // figure earns its place: if producer 0 underperforms enough not to be
        // the largest, that drops an innocent producer and leaves the flooder
        // in the population the ratio claims to describe.
        let dominant = (config.mode == "dominant" && config.active > 1).then_some(0usize);
        let excluding_dominant = dominant.and_then(|flooder| {
            let mut others: Vec<u64> = active
                .iter()
                .filter(|(producer, _)| *producer != flooder)
                .map(|(_, bytes)| *bytes)
                .collect();
            others.sort_unstable();
            let middle = median_of(&others);
            // Both operands come from the population this ratio describes.
            // Dividing by the full median made it wrong precisely when the
            // others are unequal: [1, 2, 3, 100] reported 1/3 rather than 1/2.
            (middle > 0.0).then(|| others[0] as f64 / middle)
        });
        let number = |value: Option<f64>| {
            value.map_or_else(|| "null".to_owned(), |ratio| format!("{ratio:.4}"))
        };
        println!(
            "{{\"event\":\"fairness\",\"phase\":{phase},\"active_producers\":{},\"phase_bytes\":{total},             \"min_bytes\":{lowest},\"median_bytes\":{median},\"max_bytes\":{highest},             \"min_over_median\":{:.4},\"max_over_median\":{:.4},\"starved_producers\":{},             \"dominant_producer\":{},\"excluding_dominant_min_over_median\":{}}}",
            served.len(),
            if median > 0.0 {
                lowest as f64 / median
            } else {
                0.0
            },
            if median > 0.0 {
                highest as f64 / median
            } else {
                0.0
            },
            served.iter().filter(|bytes| **bytes == 0).count(),
            dominant.map_or_else(|| "null".to_owned(), |index| index.to_string()),
            number(excluding_dominant)
        );
        assert!(
            total == 0 || lowest > 0,
            "a producer was served nothing while others were served: min={lowest} median={median} max={highest}"
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
    /// `slice[len / 2]` is the upper middle, not the median, and every fairness
    /// population is even-sized at the default 16 active producers.
    #[test]
    fn an_even_population_reports_the_middle_pair_rather_than_the_upper_one() {
        assert_eq!(
            median_of(&[1, 2, 100, 101]),
            51.0,
            "not the upper middle, 100"
        );
        assert_eq!(median_of(&[1, 2, 3]), 2.0);
        assert_eq!(median_of(&[4, 7]), 5.5, "an odd total is not floored");
        assert_eq!(median_of(&[]), 0.0);
        assert_eq!(
            median_of(&[u64::MAX, u64::MAX]),
            u64::MAX as f64,
            "no overflow"
        );
    }

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
