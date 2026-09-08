use super::{Result, allocator};
use pty_runtime::{LatencyKind, RuntimeDiagnostics};
pub fn checkpoint(phase: &str) -> Result<()> {
    let (live, peak, allocations) = allocator::snapshot();
    println!(
        "{{\"event\":\"checkpoint\",\"phase\":\"{phase}\",\"rust_requested_live\":{live},\"rust_requested_peak\":{peak},\"rust_allocations\":{allocations}}}"
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
    println!(
        "{{\"event\":\"aggregate\",\"value\":{:?}}}",
        format!("{:?}", metrics.aggregate())
    );
}
