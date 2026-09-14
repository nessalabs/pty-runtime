use super::{Result, allocator};
use pty_runtime::{CounterKind, LatencyKind, RuntimeDiagnostics};
pub fn checkpoint(phase: &str, diagnostics: Option<&RuntimeDiagnostics>) -> Result<()> {
    let gauges = diagnostics.map(RuntimeDiagnostics::aggregate);
    let live_readers = gauges
        .as_ref()
        .map_or("null".to_owned(), |g| g.live_readers.to_string());
    let scratch = gauges.as_ref().map_or("null".to_owned(), |g| {
        g.reader_scratch_allocated_bytes.to_string()
    });
    let (live, peak, allocations) = allocator::snapshot();
    println!(
        "{{\"event\":\"checkpoint\",\"phase\":\"{phase}\",\"rust_requested_live\":{live},\"rust_requested_peak\":{peak},\"rust_allocations\":{allocations},\"live_readers\":{live_readers},\"reader_scratch_allocated_bytes\":{scratch}}}"
    );
    if std::env::var_os("PTY_RELEASE_CENSUS").is_some() {
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        assert_eq!(line, "continue\n");
    }
    Ok(())
}
pub fn diagnostics(metrics: &RuntimeDiagnostics) {
    for kind in LatencyKind::ALL {
        let data = metrics.snapshot(kind);
        let number =
            |value: Option<u64>| value.map_or_else(|| "null".to_owned(), |value| value.to_string());
        println!(
            "{{\"event\":\"latency\",\"boundary\":\"{kind:?}\",\"successes\":{},\"failures\":{},\"unavailable\":{},\"p50_us\":{},\"p95_us\":{},\"p99_us\":{},\"max_us\":{},\"bucket_width_us\":100,\"buckets\":{:?}}}",
            data.samples(),
            data.failures,
            data.unavailable,
            number(data.percentile_upper_us(50)),
            number(data.percentile_upper_us(95)),
            number(data.percentile_upper_us(99)),
            data.maximum_us,
            data.buckets
        );
    }
    let aggregate = metrics.aggregate();
    println!(
        "{{\"event\":\"aggregate\",\"live_readers\":{},\"reader_scratch_allocated_bytes\":{},\"value\":{:?}}}",
        aggregate.live_readers,
        aggregate.reader_scratch_allocated_bytes,
        format!("{aggregate:?}")
    );
    // ADR 0004 allows a latency miss where "an explicit overload outcome
    // identifies the rejected operation". The runtime has counted exactly that
    // all along - `InputSaturation` is admission rejections, `OutputBackpressure`
    // is lossless admission attempts that had to wait - but the only place they
    // reached the record was as unlabelled positions inside a Debug string,
    // which is not an outcome anyone can evaluate a gate against.
    //
    // Naming them does not decide whether the escape applies; it makes the
    // question answerable from the artifact instead of from the source.
    let saturation = aggregate.count(CounterKind::InputSaturation);
    let backpressure = aggregate.count(CounterKind::OutputBackpressure);
    let gaps = aggregate.count(CounterKind::ObserverGaps);
    let gap_bytes = aggregate.count(CounterKind::ObserverGapBytes);
    println!(
        "{{\"event\":\"overload_outcome\",\"input_admission_rejections\":{saturation},         \"output_backpressure_events\":{backpressure},\"observer_gaps\":{gaps},         \"observer_gap_bytes\":{gap_bytes},\"any_operation_refused_or_delayed\":{},         \"scope\":\"Counted by the runtime, not inferred. Whether these satisfy ADR 0004's overload escape for a latency miss is a reading of that ADR, which this fixture does not make.\"}}",
        saturation > 0 || backpressure > 0
    );
}
