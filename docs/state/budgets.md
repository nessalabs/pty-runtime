---
id: "budgets"
title: "Interpret capacity and diagnostics"
kind: "guide"
status: "reference"
summary: "Quota snapshots describe logical ownership, not total process memory or demonstrated scale."
sources: ["crates/domain/src/session/mod.rs", "crates/domain/src/projection/options.rs", "crates/application/src/runtime/quota.rs", "crates/application/src/diagnostics/mod.rs", "src/runtime.rs"]
sourceRevision: "c68837e2b7bf1c9009e1a4587a1a58107ec0425c"
diagramLinks: {}
parent: "pty"
---

# Find who still owns a capacity reservation

A completed session can still occupy a registry slot until explicitly forgotten.
An old handle can retain its old context, and returned views/checkpoints/events
can retain memory permits after the session closes. Capacity refusal therefore
needs an ownership check, not an assumption that the runtime leaked or should
silently evict another session. Drop no-longer-needed observations and forget
finished IDs deliberately.

RuntimeOptions provides shared ceilings; SessionOptions supplies local limits.
Replay, parser staging, input bytes, input operations, observers, native state,
views, checkpoint pins and disk ciphertext have separate reservations. Defaults
are finite policy choices, not measured 512-session support. Dedicated readers,
Guardian helpers, OS descriptors, child memory, stacks and allocator retention
must be counted separately from logical byte budgets.

`resources()` reports current reservations. Optional RuntimeDiagnostics must be
installed before spawning the measured population. Fixed histograms and aggregate
counters retain no payloads or session labels. Concurrent snapshots are approximate;
read settled snapshots after shutdown for totals. Reset only after producers stop
and measured work settles; reset does not pause the runtime.

Cancellation dispatch timing requires actual verified target acknowledgements,
not an inferred signal from enqueue or process exit. Missing acknowledgements
produce incomplete observations rather than successful latency samples. The
existing performance harnesses own workload-specific evidence; this atlas makes
no speedup or capacity claim and adds no model of timing or OS resident memory.
