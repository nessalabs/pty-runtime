use super::{
    DEADLINE, Result, allocator, config::Config, observe::Observer, phase, population::Population,
    report, resources, sink::StalledSink,
};
use pty_runtime::{LatencyKind, RuntimeDiagnostics};
use std::time::{Duration, Instant};
pub async fn execute(config: Config) -> Result<()> {
    println!(
        "{{\"event\":\"start\",\"pid\":{},\"configuration\":{:?},\"measurement_clock\":\"monotonic\",\"fixture_threads_per_child\":2,\"fixture_control_fds_per_child\":2,\"transient_cancel_sessions\":1}}",
        std::process::id(),
        format!("{config:?}")
    );
    report::checkpoint("baseline")?;
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
    report::checkpoint("ready")?;
    if config.warmup > 0 {
        phase::run(&mut population, &mut observers, &config, 0, config.warmup).await?;
    }
    diagnostics.reset_quiescent();
    allocator::reset_peak_quiescent();
    report::checkpoint("measurement_start")?;
    let result = phase::run(&mut population, &mut observers, &config, 1, config.seconds).await?;
    report::diagnostics(&diagnostics);
    let rtt = result.fixture_rtt.snapshot(LatencyKind::InputDispatch);
    println!(
        "{{\"event\":\"fixture_rtt\",\"successes\":{},\"p50_us\":{:?},\"p99_us\":{:?},\"max_us\":{}}}",
        rtt.samples(),
        rtt.percentile_upper_us(50).unwrap_or(0),
        rtt.percentile_upper_us(99).unwrap_or(0),
        rtt.maximum_us
    );
    println!(
        "{{\"event\":\"throughput\",\"offered_bytes_per_second\":{},\"accepted_bytes\":{},\"driver_completion_seconds\":{},\"accepted_bytes_per_second\":{},\"target_seconds\":{}}}",
        config.rate,
        result.bytes,
        result.seconds,
        result.bytes as f64 / result.seconds,
        config.seconds
    );
    resources::report("settled", &population.runtime.resources(), false);
    if let Some(sink) = &sink {
        sink.report();
    }
    report::checkpoint("measurement_end")?;
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
                "attached" | "dominant" | "stalled-sink"
            ) {
                assert_eq!(observer.gaps, 0, "fast observer fell behind");
            }
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
    report::checkpoint("closed")?;
    println!("{{\"event\":\"complete\"}}");
    Ok(())
}
