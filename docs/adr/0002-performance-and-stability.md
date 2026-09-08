# ADR 0002: Performance, memory budgets, and stability

**Status: proposed for review.** Resource and acceptance targets apply to the
future Rust runtime; the cited experiments have narrower scopes.

Treat predictable resource use, responsiveness under load, and correct process
ownership as core requirements of the headless PTY backend.

Use the [PTY speed and memory experiments](../experiments/0001-pty-speed-and-memory.md) to guide the
first design. They compare PTY transport primitives and memory reclamation;
they do not establish the costs of Ghostty, child processes, Tokio, or the full
session API. Keep further investigation proportional to an unresolved design
choice rather than expanding into a general benchmark project.

## Decision

Choose dedicated readers as the initial default, prioritizing under-load latency
at an accepted per-session memory cost. Experiment 0003 measured Linux dedicated
readers at 12.94 ms probe p99 versus 45.32 ms for four shared workers, with similar
throughput and about 1.4 MiB extra owner RSS across 128 active producers. These
are transport-only results; the probe does not establish runtime control latency.
Keep active, idle, and mixed populations in qualification and compare CPU,
throughput, latency, and memory together. Retain shared readers as experiment
comparators and keep the implementation strategy replaceable. The earlier serial
feeder measurements remain historical evidence, not the current default decision.

The [architecture plan](0001-pty-runtime.md) owns implementation order and the
public session contract. This ADR owns resource and qualification policy;
[ADR 0004](0004-integration-and-release-qualification.md) defines concrete
integration scenarios and release gates. Experiment reports own measured results
and their limitations.

The [native experiment](../experiments/0002-native-terminal-parking.md) additionally
supports complete terminal parking and reclaimable allocation as design
requirements. Compression lowered 128 filled models from about 817 MiB charged
footprint to 59 MiB or 338 MiB depending on content. An experimental allocator
left about 3.8 MiB after disk parking. These whole-fixture measurements exclude
PTYs, children, raw replay, and clients, and include an approximately 1.7 MiB
baseline. Do not fold them into the idle-control budget or claim integrated
capacity from them.

## Memory objective for hundreds of sessions

Target at most 4 KiB of runtime-owned control state for an idle session with no
queued input, retained output, or optional projection. A session's descriptor,
identity, cursor positions, status, and activity metadata belong in that budget.
Account separately for each live PTY's dedicated reader stack and bounded scratch
buffer, including quiet sessions. Bound their total through admission. Avoid a
separate waiter/timer thread per session and preallocated MiB of replay.

The owner-process budget is:

`dedicated reader stacks/scratch + shared control workers/pools + session control state + queued/retained bytes + terminal models + allocator overhead`

The machine also pays for each kernel PTY and child process. Total memory cannot
be independent of session count. The objective is a small unavoidable per-session
term and bounded shared expensive resources: 500 times 4 KiB is about 1.95 MiB of
control metadata alone. Dedicated reader stacks/scratch scale additionally with
live-session count. Report reserved virtual stack space and committed/resident
memory separately; 64 KiB requested fixture stacks are not a validated production
stack size. This is a budget calculation, not a measured 500-session result.

State the memory mode explicitly. Raw PTY sessions can pursue compact idle state;
Ghostty-projected sessions add a grid, parser, and optional scrollback. A complete
terminal model is not covered by the few-KiB bookkeeping target. Measure it on
the pinned native library before selecting default projection and global limits.

Measure changes in live allocations, process RSS, charged physical footprint,
virtual reservations, and threads as session count rises. Report kernel and child
memory separately when measurable; never label user-space heap measurements as
total session memory. Include allocator high-water behavior after reclamation.

## 1. Behavioral invariants

- A client disconnect cannot terminate or relaunch its workload.
- A slow reader, abandoned attachment, or stalled event sink cannot stop PTY
  draining or other sessions. When retained history is overwritten, report the
  exact replay gap.
- Preserve byte order. Never silently drop admitted input or duplicate it during
  retries. Report partial writes and ambiguous application-level consumption
  explicitly; writing to a PTY cannot prove that the child consumed input.
- Cancellation, resize, and shutdown remain serviceable during continuous output
  and a full input queue.
- Normal shutdown reaps children and releases owned descriptors, registrations,
  observers, queued buffers, and terminal allocations. Exercise every spawn
  failure and early-exit path against the same cleanup contract.
- Reject stale cursors, invalid sizes, duplicate launches, and exhausted capacity
  with typed errors. No automatic process relaunch or unbounded retry loop.
- Keep runtime ownership in a long-lived backend process. Persistence is limited
  to client disconnection while that process survives.

## 2. Resource contracts

Require finite limits both per session and across the runtime. A per-session
buffer cap alone cannot bound a runtime with unlimited sessions or observers.

| Resource | Required policy |
| --- | --- |
| Session registry | Cap active and retained completed sessions; remove explicitly |
| Replay | Allocate on demand from a bounded shared retention budget; enforce per-session ceilings and expose eviction through cursor gaps |
| Parser staging | Reserve bounded lossless storage separately from evictable replay; backpressure the affected session at capacity |
| Input | Cap chunk size, queue bytes, queue slots, admitted operations, and admission waiters; reserve capacity before copying |
| Output reads | Cap observers, outstanding read operations, and bytes returned per call; avoid per-observer output queues |
| Terminal state | Cap cells, scrollback, query responses, parser-related limits exposed by Ghostty, and snapshot output; disable image storage initially |
| Snapshots | Cap concurrent requests and response bytes; release admission when a request completes or is cancelled |
| Checkpoint storage and transfer | Apply per-session and global limits to encrypted stored bytes, temporary encoding/restore memory, pending commits, immutable pins, and abandoned data; enforce the same runtime bounds with the built-in disk store or an injected provider |
| Event forwarding | Bound pending publications and retries; a sink cannot retain an unlimited backlog inside the PTY runtime |
| Lifecycle control | Reserve a bounded control path independent of input/output admission; coalesce redundant cancellation requests |
| Metadata | Bound diagnostics, IDs, completed-session records, and retry bookkeeping |
| Workers and descriptors | Admit one dedicated reader per live PTY; bound shared control/reaper workers, per-reader stacks/scratch, and descriptors before allocation |

Document separately: logical retained bytes, allocated buffer capacity, Ghostty
page overhead, thread stacks, OS PTY buffers, and total process RSS. Ghostty's
scrollback byte setting is approximate at page granularity. Buffers handed to a
caller become caller-owned memory; the library can bound each response and its
own outstanding operations, not how many responses that caller keeps forever.

## 3. Execution and scheduling

Build and qualify dedicated readers for large active, idle, and mixed sets.
Measure quiet-reader overhead, stack safety, interruption, shutdown, and thread
limits as well as saturated throughput and latency. Keep shared readiness pools
as comparators using the same buffers, terminal parsing, and workload.
Qualify bounded dynamic placement as described in
[ADR 0003](0003-session-parking-and-state-transfer.md) before enabling it. Fixed
reader measurements do not establish handoff correctness or integrated performance.

Both a blocked read and a blocked readiness wait can sleep without polling. Avoid
busy loops, periodic reads of idle PTYs, and full snapshots on every output
chunk. Trigger screen extraction only on request with bounded admission. A
readiness notification is permission to attempt I/O, not a reserved byte count;
handle short reads, EINTR, and EAGAIN, and clear readiness correctly.

Batch reads and writes within finite byte/work budgets and yield between batches.
One continuously readable session must not monopolize an executor worker. Keep
global registry locks out of I/O, terminal parsing, snapshot formatting, and event
publication. Serialize each session's terminal mutations and PTY writes without
serializing unrelated sessions.

Measure the raw PTY path, incremental Ghostty parsing, snapshot extraction, and
event publication separately and together. Keep callbacks synchronous, bounded,
and nonreentrant. If a native operation cannot meet a cooperative scheduling
budget, evaluate dedicated bounded workers before accepting that operation on
the async executor.

Track copies, allocations, syscalls, wakeups, and lock contention before adding
optimizations. Minimize unnecessary copies and formatting while preserving
simple ownership and auditable FFI safety. Any optimization must retain the
correctness and resource tests.

### Parking and physical memory reclamation

Terminal parking eligibility starts at 60 seconds without PTY output or other
model mutation. Keep readiness and exit monitoring armed. Already-encoded input
and compatible snapshot attachment need not wake a parked model; output and
operations requiring live state do. Reclaim spare observer buffers independently,
while retaining the dedicated reader in the initial implementation. Use one bounded shared
expiry mechanism and avoid unbounded timer churn or a timer thread per PTY.

Waiting alone does not release buffers or thread resources. The initial cache
experiment also shows that dropping a Rust allocation may leave charged process
memory high. Evaluate releaseable pages for large owned replay/cache pools when
prompt physical reclamation matters, including allocation, mapping, fragmentation,
and wake costs. The experiment used synthetic caches; it does not authorize
discarding replay history or live terminal state without the specified semantics.

For projected sessions, evaluate Ghostty's incremental history compression with
bounded work outside the hot I/O path. Serialize it with writes/resizes/snapshots.
Do not use an unbounded full compression pass on an executor worker. Preserve
active screen, modes, parser continuation, and terminal reply behavior while cold,
either resident or in a complete restorable checkpoint. Apply the storage,
ownership, and cursor contracts in ADR 0003 before releasing a live model.

Measure time to READY, complete history, and first output after wake separately.
Include encoding CPU, in-memory compression, storage I/O, temporary peak memory,
and repeated transitions. Native codec timing alone is not disk wake latency.
When raw delivery is decoupled from parsing, report raw availability and model
freshness separately; retain the existing ordered-feed measurement for comparison.

## 4. Initial qualification targets

The following are proposed acceptance targets for the recorded Apple Silicon
macOS baseline, not measured performance claims. Confirm the baseline and test
parameters in the native feasibility stage, and record any target revision with
its reason before implementation is declared complete.

Use release builds with the pinned native dependency. Record host model, CPU,
RAM, OS, Rust/Zig versions, build flags, terminal dimensions, all configured
limits, and competing system load. Separate child-fixture cost from runtime cost.

First qualify 1, 32, and 128 sessions to establish marginal memory and worker
cost. Qualify 500 sessions on a host with sufficient PTY/process capacity before
making that capacity claim; do not change a user's global PTY limits for a test.
This macOS host reports a system-wide PTY limit of 511, which must also serve
other applications.

Baseline workload for the integrated backend: 64 resident sessions, 16 actively emitting a combined
10 MiB/s, 80-by-24 grids, a 1 MiB replay cap per session, and a documented finite
Ghostty scrollback limit. Exercise a reproducible mix of ASCII, split UTF-8,
cursor movement, erasure, color sequences, and terminal queries. Run both
attached and detached cases, then a case where one session produces most output.

Retain that controlled workload as a latency reference. Also qualify 128
independently active producers and mixed populations of 128 sessions, sweeping
offered rate per producer to locate capacity and fairness limits. Qualify 500
active sessions on suitable hosts before making that capacity claim. A strong
idle-memory result cannot substitute for active-session throughput and latency.

| Measurement | Initial acceptance target |
| --- | --- |
| Input dispatch | p99 <= 20 ms from admitted input to completed PTY write when the child endpoint remains writable; report admission delay separately |
| Output availability | p99 <= 20 ms from host endpoint read completion to availability to a ready observer, including ordered terminal feed |
| Control responsiveness | p99 <= 100 ms from an admitted cancel/resize request to its OS operation; process exit has its separate configured grace period |
| Idle runtime CPU | <= 1% of one CPU core, averaged over 60 seconds with 64 idle sessions and idle fixtures; report thread/wakeup counts |
| Sustained throughput | Drain the 10 MiB/s qualification workload without unexpected byte loss or starvation; report CPU, memory, and each stage's throughput |
| Bounded retention | Replay and admitted queues remain within their configured caps throughout reader/sink stalls |
| Idle control memory | <= 4 KiB per session plus a measured shared baseline; dedicated reader stacks/scratch, projected terminal, retained bytes, kernel, and child costs reported separately |
| Cleanup | After completion and explicit forgetting, child/descriptor/worker counts return to their measured baseline and retained runtime objects are released |

Timestamp within the Rust fixture and runtime using a monotonic clock. Report
p50, p95, p99, maximum, sample counts, and timeouts. Do not omit failed or stalled
operations from the report. Output availability starts at host read completion;
report a separate fixture round-trip measurement to include OS scheduling and
PTY buffering. Backpressured writes are a separate scenario whose requirement
is bounded admission and responsive cancellation.

After warm-up, collect at least five 60-second runs per throughput/latency case.
Sweep session count, output rate, chunk size, observer count, and terminal size
to identify capacity limits and the point where typed admission rejection begins.
Record a reviewed baseline for each supported host. Treat a repeatable regression
greater than 10% in throughput or p99 latency as a release review failure pending
explanation and acceptance; shared CI timing alone is not sufficient evidence.

## 5. Stability qualification

Use synthetic Rust fixtures and deterministic seeds. Each failing scenario must
record enough non-sensitive configuration to reproduce it.

| Scenario | Required result |
| --- | --- |
| 10,000 spawn/exit/cancel cycles | No duplicate launches, zombies, unreleased descriptors, or growing session metadata after explicit removal |
| 100,000 attach/detach operations | Workload identity preserved; cursors advance correctly; abandoned observers release resources |
| Concurrent lifecycle races | Cancel vs exit, resize vs exit, attach vs eviction, and owner shutdown vs spawn preserve typed outcomes and ownership |
| Slow and disconnected consumers | The process and other sessions continue; retained bounds hold; reconnect reports gaps precisely |
| Child stops reading | Input admission remains bounded and cancellation bypasses a saturated queue |
| Continuous output and terminal-query flood | Control latency remains within the qualification target or the documented overload policy is triggered |
| Snapshot and event-sink pressure | Bounded operations; no deadlocks, unbounded allocations, or blocking of unrelated sessions |
| Cold/warm transitions | Preserve readiness, process identity, byte order, parser state and cursor gaps; report reclaimed bytes, charged footprint, and first-byte latency |
| Failed spawn and resource exhaustion | Failure releases partial allocations; surviving sessions remain serviceable |
| 12-hour mixed-load soak | Stable resource plateaus after warm-up, no accumulating children/descriptors/workers, correct byte/gap accounting, and no unexpected task failures |
| Abrupt owner termination | Record actual OS behavior and descendant limits; never report durable recovery from an output snapshot |

Test arbitrary chunk boundaries, malformed/truncated escape sequences, excessive
sequence sizes, and randomized Unicode/VT input against the real Ghostty wrapper.
Fuzz bounded byte-feed, resize, and snapshot sequences. Run native memory/error
instrumentation where supported, recording tool limitations and failures.

Recoverable I/O, sink, and task failures should be confined to the affected
operation or session. A native-library abort or memory-safety failure can end the
owner process; an in-process wrapper cannot promise crash isolation from native
code. Qualification must expose this boundary and investigate native faults.

## 6. Diagnostics and release evidence

Expose bounded aggregate counters for active sessions/observers, admitted bytes,
retained bytes, gaps, queue saturation, bytes read/written, failed operations,
cancellation escalation, and cleanup outcomes. Allow caller-owned instrumentation
without requiring a logging, metrics, GUI, or cloud service.

Metrics and diagnostics must exclude command arguments, environment values,
input/output contents, terminal text, and unbounded session-ID labels. Detailed
content inspection remains an explicit caller operation.

Publish the test workload, exact commands, measurements, resource configuration,
and operating-system coverage with the validation report. Unit tests, configured
CI, cross-compilation, and executed stress/soak checks are distinct evidence.
Performance targets and stability gates must be assessed together before release.
