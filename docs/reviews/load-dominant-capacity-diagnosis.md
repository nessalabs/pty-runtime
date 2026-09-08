# Dominant ordinary-load capacity failure: read-only diagnosis

The frozen candidate-2 fifth dominant trial remains an open release gap. The most likely propagation site is projected resize admission, and a transient exhausted per-session staging-slot quota is a plausible explanation. The retained evidence does **not** identify the rejected quota or prove this explanation; no runtime fix is selected.

## Evidence and source identity

Reviewed `docs/verification/release/load-observers-candidate2/dominant-5.jsonl` against `work/release-candidate-2`. Its binary SHA-256 is `898cfc9cb198947557e8c4076f6e5e1ceb2c49b6370ce079670332f29189a811`, source HEAD is `52ad04c3519616e6cedea9ac8707406970a40ed7`. All 11 relevant frozen files match the trial's exact source inventory. Hashes, event counts and sampled maxima are retained in `docs/verification/load-dominant-capacity-diagnosis/evidence.json`.

Configuration: 64 resident projected sessions, 16 active, aggregate paced 10 MiB/s, dominant producer receives 90%, 4093-byte chunks, one observer each, 80×24, ten-second warmup and sixty-second measurement. Warmup has all 64 producer ends and done summaries. Measurement has all 64 producer starts but no producer ends. The final checkpoint is `measurement_start`; stderr is exactly `Error: Projection(Capacity)`. Process exit is 1, not driver kill or a recorded panic. No throughput, final ledger or completion event exists for this failed trial.

The one-second shared-budget samples show maxima of 4 staging slots out of 16,384, 4,093 staging bytes out of 67,108,864, zero requests out of 4,096, and 536,870,912 native reservation bytes out of 1,073,741,824. These are sampled shared reservations, not rejection-time facts, local-session quotas, high-water marks or actual native allocations. Their low values cannot exclude a transient local shortage between samples.

## Call-path narrowing

All source line references below refer to the frozen candidate, not the subsequently edited harness.

- `examples/release_load_support/phase.rs:130–132` can return `RuntimeError::Projection(Capacity)` from either the synchronous `resize_projected(size)?` admission or the admitted operation's awaited outer `Err` converted with `RuntimeError::from`.
- `crates/application/src/runtime/session_projection.rs:51` forwards to `ProjectionCoordinator::resize_timed`. Admission (`projection/admission.rs:125–133`) reserves a request ticket, then one shared/local staging slot, then checks existing failure and updates activity. `staging(0)` does not reserve payload bytes. Both request and staging leases collapse shared versus local quota rejection into the same `ProjectionError::Capacity` (`budgets.rs:120–138`).
- Default local staging capacity is 256 slots and local outstanding request capacity is 32 (`domain/src/projection/options.rs:103–104`); output and controls share staging slots. This harness awaits one resize at a time, making ordinary accumulated request saturation less plausible than a transient backlog of the dominant producer's output. Worker execution waits for authoritative reply completion before consuming more events (`projection/worker.rs:21`, `native.rs:25–74`), so a temporary backlog is possible without exceeding shared sampled limits. This is a mechanism, not evidence that it occurred.
- `accepting` may also return a previously recorded plain capacity failure. An admitted resize may receive a failure when queued controls are rejected by `ProjectionCoordinator::fail` (`worker.rs:124–148`), or from the checked control-generation overflow in `native.rs:175`. The finite trial cannot realistically exhaust the u64 generation counter; the log still lacks a status snapshot to distinguish rejection from a permanent projection failure.
- An actual resize OS/model failure is returned inside `ResizeOutcome`, which this harness asserts at `phase.rs:133`; that would have different panic output. Terminal adapter errors retain `ProjectionError::Terminal(...)` (`domain/src/projection/mod.rs:80–83`) rather than becoming plain capacity. Therefore this log does not support diagnosing a native allocation or malformed-input crash.
- Ordinary observer reads return replay/cursor/runtime errors, input admission returns runtime/process capacity, and the transient cancellation probe is raw. Those paths do not explain this exact projection error category during the active phase as directly as projected resize does.

## Minimal next diagnostic change: fixture only, not implemented

Split the existing projected-resize expression into its already-existing three error boundaries: synchronous admission, timeout while awaiting, and outer completion error. Preserve the same operation, deadline, return conversion and failure result; do not retry, increase quotas, skip a control or reinterpret capacity as success. On the error path only, emit one structured `operation_failure` record containing:

1. Operation `resize_projected`, stage `admission` / `timeout` / `completion`, phase, producer index, probe sequence, requested grid, monotonic timestamp and elapsed phase time, plus the original portable error.
2. The immediate result of that child's public `projection_status()` and `status()` calls, preserving diagnostic-call failure separately. Projection status supplies residency, failure, published/processed positions and control generation; record the raw fields without inventing a local queue depth from byte lag.
3. One immediate public `runtime.resources()` snapshot, clearly labeled a concurrent shared snapshot. Emit before returning/unwinding and before population shutdown. Use diagnostic output that cannot replace the original error with a secondary assertion.

This fixture-only context can distinguish rejected admission from an already-admitted operation failing, and distinguish a still-healthy projection from persistent failure. It cannot prove which shared/local request or staging quota rejected the admission: the public snapshot exposes shared usage only, and a later sample can race with releases. Exact quota attribution would require separately reviewed internal rejection diagnostics if the fixture evidence remains insufficient; do not infer it from low sampled totals or change runtime behavior now.

No source, runtime, native, workload or frozen-candidate files were edited. This review only read ordinary budget/control paths and wrote review evidence. No Linux workload, malformed-input/native-crash investigation, test execution, rebuild or reproduction was performed. Preserve the failed fifth trial as a failed repeat; prior passing repeats do not discharge it.

## Subsequent fixture instrumentation

At the coordinating task’s request after checkpoint `5373d8e`, the proposed fixture-only error context was implemented in canonical `examples/release_load_support/resize.rs` and connected from `phase.rs`. See `docs/verification/load-resize-failure-diagnostics/` for exact scope, tests, Clippy and source identity, and `docs/reviews/load-resize-diagnostic-independent.md` for independent source review. This does not alter the frozen trial or establish its underlying cause. No diagnostic load reproduction has been executed by this author.
