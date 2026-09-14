use super::{
    DEADLINE, Result, allocator, config::Config, observe::Observer, phase, population::Population,
    report, resources, sink::StalledSink,
};
use pty_runtime::{LatencyKind, RuntimeDiagnostics};
use std::time::{Duration, Instant};
pub async fn execute(config: Config) -> Result<()> {
    println!(
        "{{\"event\":\"start\",\"pid\":{},\"configuration\":{:?},\"measurement_clock\":\"monotonic\",\"projection_staging_slots_per_session\":{},\"fixture_threads_per_child\":2,\"fixture_control_fds_per_child\":2,\"fixture_output_fds_per_child\":1,\"transient_cancel_sessions\":1}}",
        std::process::id(),
        format!("{config:?}"),
        if config.raw {
            "null".to_owned()
        } else {
            config.staging_slots.to_string()
        }
    );
    report::checkpoint("baseline", None)?;
    let diagnostics = RuntimeDiagnostics::new();
    let mut population = Population::new(&config, diagnostics.clone())?;
    for child in &population.children {
        println!(
            "{{\"event\":\"child\",\"producer\":{},\"pid\":{}}}",
            child.producer, child.pid
        );
    }
    let mut observers = Vec::with_capacity(config.sessions);
    for child in &population.children {
        let count = if config.mode == "detached" {
            0
        } else {
            config.observers
        };
        observers.push(
            (0..count)
                .map(|_| Observer::new(&child.session, child.producer))
                .collect::<Result<Vec<_>>>()?,
        );
    }
    let sink = if config.mode == "stalled-sink" {
        Some(StalledSink::start(&population.children[0].session).await?)
    } else {
        None
    };
    resources::report("ready", &population.runtime.resources(), false);
    report::checkpoint("ready", Some(&diagnostics))?;
    if config.warmup > 0 {
        phase::run(&mut population, &mut observers, &config, 0, config.warmup).await?;
    }
    diagnostics.reset_quiescent();
    allocator::reset_peak_quiescent();
    report::checkpoint("measurement_start", Some(&diagnostics))?;
    let result = phase::run(&mut population, &mut observers, &config, 1, config.seconds).await?;
    report::diagnostics(&diagnostics);
    // ADR 0004 asks that reference-state comparisons pass. Unit tests already
    // feed an independent engine the same bytes and require an equal view, but
    // only on a quiet runtime. What that leaves unproven is the one thing this
    // matrix exists to stress: whether the projection is still *correct* when
    // it has been under pressure, rather than merely still keeping up.
    //
    // The comparison is affordable because the ADR's offered rate is combined
    // across producers, so one producer's stream is about 37 MiB rather than
    // the 600 MiB the session population moves between them.
    if config.mode == "reference" {
        reference_state(&population, &config).await?;
    }
    let rtt = result.fixture_rtt.snapshot(LatencyKind::InputDispatch);
    let number = |value: Option<u64>| value.map_or_else(|| "null".to_owned(), |v| v.to_string());
    println!(
        "{{\"event\":\"fixture_rtt\",\"successes\":{},\"failures\":{},\"unavailable\":{},\"p50_us\":{},\"p95_us\":{},\"p99_us\":{},\"max_us\":{},\"bucket_width_us\":100,\"buckets\":{:?}}}",
        rtt.samples(),
        rtt.failures,
        rtt.unavailable,
        number(rtt.percentile_upper_us(50)),
        number(rtt.percentile_upper_us(95)),
        number(rtt.percentile_upper_us(99)),
        rtt.maximum_us,
        rtt.buckets
    );
    println!(
        "{{\"event\":\"throughput\",\"offered_bytes_per_second\":{},\"producer_mode\":\"{}\",\"accepted_bytes\":{},\"driver_completion_seconds\":{},\"producer_window_seconds\":{},\"accepted_bytes_per_second\":{},\"target_seconds\":{}}}",
        if config.mode == "saturation" {
            "null".to_owned()
        } else {
            config.rate.to_string()
        },
        if config.mode == "saturation" {
            "unpaced"
        } else {
            "paced"
        },
        result.bytes,
        result.completion_seconds,
        result.seconds,
        result.bytes as f64 / result.seconds,
        config.seconds
    );
    resources::report("settled", &population.runtime.resources(), false);
    if let Some(sink) = &sink {
        sink.report();
    }
    report::checkpoint("measurement_end", Some(&diagnostics))?;
    // Exact lifetime-zero reconnect: missing bytes must be one or more explicit gaps,
    // and every retained suffix byte must match its absolute deterministic offset.
    for (index, child) in population.children.iter().enumerate() {
        if observers[index].is_empty() {
            observers[index].push(Observer::new(&child.session, index)?);
        }
        for observer in &mut observers[index] {
            let deadline = Instant::now() + DEADLINE;
            while observer.cursor < child.total {
                observer.poll()?;
                assert!(Instant::now() < deadline);
            }
            assert_eq!(observer.cursor, child.total);
            assert_eq!(observer.bytes + observer.gaps, child.total);
            println!(
                "{{\"event\":\"ledger\",\"producer\":{index},\"verified_bytes\":{},\"gap_bytes\":{},\"total_bytes\":{}}}",
                observer.bytes, observer.gaps, child.total
            );
            if matches!(
                config.mode.as_str(),
                "attached" | "reference" | "dominant" | "stalled-sink"
            ) {
                assert_eq!(observer.gaps, 0, "fast observer fell behind");
            }
            // `detaching` is deliberately absent above. Once every observer has
            // gone, the final accounting attaches a fresh one at offset zero,
            // and a 1 MiB retention cap cannot still hold half a minute of
            // output at 10 MiB/s — so a gap is the correct answer, not a
            // failure. What must hold is that the gap is *exact*, which the
            // `bytes + gaps == total` assertion above already requires of every
            // mode. That is the ADR 0004 claim this case exists to test: after
            // all observers detach, a later attach is told precisely what it
            // missed rather than being given silence or the wrong bytes.
        }
    }
    if let Some(sink) = sink {
        sink.stop().await?;
    }
    drop(observers);
    let children = std::mem::take(&mut population.children);
    for child in children {
        population.finish(child, false).await?;
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    resources::report("forgotten", &population.runtime.resources(), true);
    drop(population);
    report::checkpoint("closed", Some(&diagnostics))?;
    println!("{{\"event\":\"complete\"}}");
    Ok(())
}

/// Compare one session's projected state against an independent engine fed the
/// same bytes, after that session has spent a full measurement phase under load.
///
/// The byte stream is regenerated from `payload` rather than replayed from the
/// observer, deliberately: retention is capped at 1 MiB per session, so an
/// observer cannot supply the whole history, while the payload is a pure
/// function of offset and producer and can.
#[cfg(feature = "ghostty")]
async fn reference_state(population: &Population, config: &Config) -> Result<()> {
    use pty_runtime::ports::ITerminalFactory;
    use pty_runtime::terminal::{TerminalConfig, TerminalSize};
    use pty_runtime_infrastructure::terminal::GhosttyTerminalFactory;
    let Some(child) = population.children.first() else {
        return Ok(());
    };
    let size = TerminalSize::new(config.cols, config.rows)
        .map_err(|_| std::io::Error::other("invalid grid"))?;
    let terminal_config = TerminalConfig::new(size);
    let feed_bytes = terminal_config.feed_bytes;
    let mut reference = GhosttyTerminalFactory
        .create(terminal_config)
        .map_err(|error| std::io::Error::other(format!("reference engine: {error:?}")))?;
    let began = Instant::now();
    let mut offset = 0u64;
    let mut buffer = vec![0u8; feed_bytes];
    while offset < child.total {
        let take = feed_bytes.min((child.total - offset) as usize);
        super::payload::fill(&mut buffer[..take], offset, child.producer);
        reference
            .feed(&buffer[..take])
            .map_err(|error| std::io::Error::other(format!("reference feed: {error:?}")))?;
        offset += take as u64;
    }
    let projected = child
        .session
        .projected_view()
        .map_err(|error| std::io::Error::other(format!("projected view: {error:?}")))?
        .await
        .map_err(|error| std::io::Error::other(format!("projected view await: {error:?}")))?;
    let expected = reference
        .view()
        .map_err(|error| std::io::Error::other(format!("reference view: {error:?}")))?;
    let equal = projected.view() == &expected;
    println!(
        "{{\"event\":\"reference_state\",\"producer\":{},\"bytes_compared\":{},\"feed_seconds\":{:.3},\"grid\":\"{}x{}\",\"equal\":{equal}}}",
        child.producer,
        child.total,
        began.elapsed().as_secs_f64(),
        config.cols,
        config.rows
    );
    // Deliberately an assertion rather than a reported number: an unequal view
    // after load is a correctness failure, not a measurement.
    assert!(
        equal,
        "projected state diverged from an independent engine fed the same {} bytes",
        child.total
    );
    Ok(())
}

/// Without the engine there is nothing to compare against, and quietly skipping
/// would let a `reference` run report success while checking nothing.
#[cfg(not(feature = "ghostty"))]
async fn reference_state(_population: &Population, _config: &Config) -> Result<()> {
    Err(std::io::Error::other(
        "the reference mode compares against a Ghostty engine and needs that feature",
    )
    .into())
}
