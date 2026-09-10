# Independent diagnostics DDD and organization review

Reviewed 2026-09-08 on macOS arm64 by an independent specialist who did not author
this diagnostics/acknowledgement implementation. This is the requested DDD and
Clean Code/SOLID review, separate from the process behavioral review. No open
P1/P2 findings in this scope. No production changes were made by this reviewer.

## Dependency direction and contracts

`application/diagnostics` owns fixed, unlabeled observational counters, timing
owners and snapshot values. It introduces no dependency beyond domain/std and
contains no process/native types. Domain continues to own validated admission
limits and state policy. Quota snapshots copy existing reservations rather than
establishing a competing admission policy or deriving capacity from metrics.
The returned values explicitly distinguish logical reservations from RSS and
consumer-retained memory from current activity. The shared concrete diagnostics
object is appropriate here: it is a bounded in-memory observation mechanism, not
a replaceable external service requiring another interface.

Infrastructure owns actual read/write/resize/signal boundaries. Optional process
port methods preserve injected-adapter compatibility. The timed resize fallback
marks unavailable explicitly. `Timing` owns precisely one completion outcome;
Drop records an unfinished operation as failure, avoiding falsely successful
percentiles. Failure recording is bounded and does not execute user callbacks.
Counters never determine session behavior. Actual monotonic `Instant` values stay
in the application/adapter layer, separate from domain state transitions.

## Cohesion, ownership and failure paths

The four diagnostics modules separate histogram arithmetic, aggregate counters,
public timing/API ownership, and resource snapshot shapes. Runtime call sites
record admission/publication facts; process call sites record OS outcomes. There
is no registry scan, per-session metric label, service locator, or unbounded
sample ledger. All reviewed production files remain under 350 nonblank lines;
`runtime/context.rs` is 342 and infrastructure `process/guardian.rs` is 345 after
formatting, so future additions to either should trigger an ownership-based split.

Active activity is released once on completion or context destruction using a
state-owned marker, while raw retention remains charged until its last context
owner is gone. Quiescent reset preserves gauges. The poisoned-context Drop path
recovers exclusive owned state and releases replay/observer reservations and
both gauges; poison does not erase allocation ownership. Process sessions retain
only the diagnostics Arc, not the runtime event/context owner, so instrumentation
does not introduce an ownership cycle. Timing transfer uses existing bounded
operation slots and leases rather than an additional asynchronous queue.

## Signal acknowledgement boundary

The shared protocol keeps wire records and generation validation within
helper/infrastructure. TERM success is emitted only after direct-child and pinned
root/foreground syscall acknowledgements. `Escalated = 18` is distinct from TERM
acknowledgement and represents successful direct-workload SIGKILL escalation;
the helper endpoint has 19 fixed per-kind slots and the host consumes that fact
once. The host request counter and `AcknowledgedWorkloadEscalations` remain
separate. Neither claims group-member acknowledgement after SIGKILL. This cleanly
separates control intent, verified native facts, and diagnostic aggregation.
Missing transport does not invent an acknowledgement. No errno, PID, raw frame,
or helper lifecycle enum enters diagnostics snapshot/domain values.

## Independent validation

After the author's formatting freeze, independently ran:

- `cargo test --locked -p pty-runtime-application --lib`: 39 passed, including
  histogram boundaries and poisoned-context ownership release.
- `cargo test --locked --features ghostty --test runtime_diagnostics`: four passed,
  including real PTY input/projection boundaries, retained completed contexts,
  exact gap counters, and acknowledged workload escalation.
- `cargo test --locked --manifest-path helpers/guardian/Cargo.toml`: three passed,
  covering bounded transport, generation validation, and partial frames/EOF.
- Read-only nonblank source inventory: every reviewed production file passed.

The coordinating reviewer owns full-gate execution and release qualification.
These focused checks do not establish all failure timing combinations, long-run
counter behavior under continuous load, supported-platform performance, or soak.
