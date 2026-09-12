# ADR 0004: Integration correctness and release qualification

**Status: qualification in progress.** Focused runtime and adapter tests have run;
the full integrated gates below remain pending. The [proof ledger](../verification/requirements.md)
distinguishes executed implementation tests from standalone experiments and
unrun release requirements.

The [architecture](0001-pty-runtime.md) defines the public contract;
[ADR 0002](0002-performance-and-stability.md) owns resource limits and numeric
targets; [ADR 0003](0003-session-parking-and-state-transfer.md) owns checkpoint
and reader-placement policy. This document defines how to verify their integration.

## Decision

Build and qualify the dedicated-reader session owner first. Add real Ghostty
projection next, then default parking with a replaceable store and its own
correctness gate. Require
executed platform evidence before declaring a target supported. Dynamic reader
switching remains a later extension and is not a gate for the dedicated baseline.

## State and completion contract

Track process state, output-drain state, terminal residency, and observer state
independently. A child can have exited while an inherited child endpoint descriptor still
keeps output open. A client can detach while the process continues. A terminal
can fail to restore while its child remains alive.

Record a real OS exit status separately from output completion. Completion must
say whether output reached normal EOF or was cut off by the configured drain
deadline. A timeout is neither proof of child exit nor proof of complete output.

Cancellation has a request phase and a wait phase. Once admitted, the termination
request survives cancellation of its caller's wait. Concurrent requests coalesce
into one escalation sequence and share the actual exit result. Shutdown uses the
same ownership rules. Never automatically resend a partially written input chunk.

After a failed restoration, preserve the child and bounded raw I/O, report that
the terminal projection is unavailable, and retain recovery material according
to the storage contract. Do not silently create an empty replacement terminal,
drop parser input, or relaunch the process. If pending parser input reaches its
cap, apply the session's explicit backpressure policy.

## Test fixtures and observations

Use small Rust child fixtures with deterministic numbered byte sequences,
checksums, terminal queries, and explicit barriers. A separate test control
channel releases barriers and acknowledges fixture progress without changing
the PTY payload. Count those fixture descriptors separately from runtime cost.

Run deterministic interleavings first, then repeat with recorded random seeds.
Give every operation a deadline and include failures and timeouts in results.
Compare delivered byte ranges with fixture output and projected state with an
uninterrupted native reference receiving the same bytes and ordered controls.
Report latency, memory, and cleanup under the same workload as correctness.

### Concurrent output and pressure

Start with ADR 0002's 64 resident sessions and 16 active producers emitting a
combined 10 MiB/s. Repeat with one producer responsible for most output, with
fast and stalled observers, after all observers detach, and with a stalled
event-stream sink. Include split UTF-8/VT sequences and terminal queries.
Use an independent child producer for each active PTY, released by a start
barrier. Sweep the offered rate per producer to locate capacity limits. Report
aggregate throughput, per-PTY progress and blocking, and input/control latency
while output is active. Do not substitute a single serial feeder or measure all
latency only after the bulk load stops.

The target also requires hundreds of active sessions: add 128 independent active
producers and 128-session mixed populations as primary qualification scenarios.
Extend to 500 active sessions where host capacity permits. The 64/16 workload is
a controlled latency reference, not the full capacity requirement.

Pass criteria:

- Every retained byte appears in order; any observer replay loss has an exact
  cursor gap. Eviction never discards input required by the authoritative parser.
- Replay, parser staging, input, snapshots, and event forwarding stay within
  their separate configured bounds, including concurrent temporary allocations.
- A full parser backlog applies backpressure only to that session. Other
  sessions, cancellation, and exit supervision continue within their contract.
- Admitted user input and terminal replies share one ordered writer. Replicated
  observers do not generate additional authoritative query replies.
- Fairness and latency meet ADR 0002's targets, or an explicit overload outcome
  identifies the rejected operation. A timeout cannot disappear from statistics.

### Cancellation, exit, and cleanup

| Scenario | Required result |
| --- | --- |
| Cooperative child; child ignoring graceful termination | Signal the intended live process groups, escalate once after the configured grace period, and collect the actual exit result |
| Child stops reading; input queue is saturated | Bound queued bytes and waiters; cancellation bypasses that queue; partial writes remain visible |
| Cancellation or resize races with exit | Preserve actual exit status, return a typed control outcome, and never signal a reused PID |
| Many callers cancel; callers abandon waits | Coalesce requests; abandoning a wait does not undo termination or orphan cleanup |
| Child exits while a descendant holds the child endpoint open | Continue bounded draining, then report either EOF or incomplete drain separately from child exit |
| Spawn fails midway; shutdown races with spawn | Reject or complete admission consistently; release partial descriptors, allocations, registrations, and owned children |

After completion and explicit removal, compare child, descriptor, worker, and
live-allocation counts with the pre-test baseline. Report allocator-retained
footprint separately from leaked live objects. Deliberately escaped descendants
remain an explicitly documented process-group limitation.

### Parking and restoration races

A checkpoint captures both the processed byte position and ordered control
position. For example, a checkpoint covering bytes `[0, 8192)` followed by output
`[8192, 8256)` must restore the checkpoint and apply those 64 bytes exactly once.
A resize consumes no output bytes, so the byte cursor alone cannot order it.

Encoding owns the native model exclusively for its synchronous call. Its callback
must accept bytes into bounded staging or fail promptly. Slow storage commit
runs outside that native ownership interval. Associate every attempt with a
session lifetime and operation generation.

If output or another model mutation arrives before storage commit, invalidate
that parking attempt and retain the live model. An old storage completion may
neither replace newer state nor revive a removed session. Before releasing the
live model, atomically verify the attempt generation, committed checkpoint, and
absence of pending activity. Bound and clean up unused checkpoint data.

Test barriers at encode completion, storage commit, model release, READY, and
each history-restoration step:

| Interleaving or failure | Required result |
| --- | --- |
| Output arrives during encode or commit | Preserve bytes in bounded staging; retain the current live model when the attempt becomes stale |
| Output arrives immediately after model release | Armed read readiness wakes restoration; coalesce restore requests; feed queued bytes once after READY |
| Input arrives while parked | Already-encoded bytes can reach the PTY; model-dependent encoding restores first; resulting output triggers normal restoration |
| Resize arrives while parked or restoring | Serialize OS/model resize with ordered output; report partial failure truthfully and preserve the control position |
| Cancel or exit occurs during restore | OS control and reaping continue; stale completion cannot publish state; native handles stay owned until in-flight FFI ends |
| Attach or detach races with snapshot-to-live transfer | Pin bounded immutable state; preserve byte/control order; require explicit resynchronization when retained events are lost |
| Encoding, encryption, or storage fails before commit | Keep the original model, report the failure, and clean up temporary data |
| A parked checkpoint is corrupt, truncated, incompatible, or unavailable | Report projection failure while preserving process ownership and bounded raw I/O; never claim successful restoration |

Exercise unfinished UTF-8, CSI, OSC and DCS sequences, alternate screen, resize,
query replies, continuation-limit exhaustion, and repeated park/restore cycles.
Compare final state with the uninterrupted reference. Report READY separately
from history completion, including pages that cannot be restored after live
mutations. Test storage capacity, cancellation, and reclamation under pressure.

Run the storage contract suite against the built-in disk store and an injected
test provider. Verify default setup without a supplied store, private directory
ownership, runtime encryption, capacity accounting, commit/read/delete behavior,
and cleanup. Inject delays, short reads, failed writes, full storage, and cancelled
commits. Changing providers must preserve the same byte/control and lifecycle
results, and a provider must never receive plaintext checkpoint contents.

## Platform qualification

Linux qualification requires execution on Linux hardware or a Linux VM. A
successful macOS run, cross-build, or CI configuration does not establish Linux
behavior. Verify controlling-terminal setup, foreground process groups,
nonblocking readiness, partial I/O, hangup/EOF handling, reaping, resize,
cancellation, and cleanup on each target. Exercise the native codec and memory
reclamation there as well, using that platform's available memory metrics.

| Target | Status | Current executed evidence | Still required |
| --- | --- | --- | --- |
| macOS arm64 | Partially exercised; not qualified | 245 repeated PTY/native fixture runs, including 128 active producers (Experiment 0003, executed 2026-09-08, `docs/experiments/data/0003/macos-metadata.json`: Apple M5, Mac17,3, Darwin 25.6.0). Native adapter tests, 11 native tests on 2026-09-08 (`scripts/native/README.md`). Coverage measurement on macOS 15 / aarch64 (`docs/todo/release-blockers.md`) | Rust integration, race/stress tests, performance qualification, and soak |
| macOS x86_64 | **Unqualified — no executed evidence** | None. No run on this target is recorded anywhere in the tree, and no workflow selects a macOS x86_64 runner. The pinned `x86_64-macos` Zig archive in `experiments/dependencies.json` is a download pin, not a build or a run | Native build and executed behavioral/resource qualification |
| Linux arm64 | **Unqualified — no executed evidence** | None. No run on this target is recorded anywhere in the tree, and no workflow selects a Linux arm64 runner. The pinned `aarch64-linux` Zig archive in `experiments/dependencies.json` is a download pin, not a build or a run | Native build and executed behavioral/resource qualification |
| Linux x86_64 | Partially exercised; not qualified | 245 repeated PTY/native fixture runs, including 128 active producers (Experiment 0003, executed 2026-09-08, `docs/experiments/data/0003/linux-metadata.json`: Xeon Skylake VM, Linux 6.8.0-117, glibc 2.39) | Rust integration, race/stress tests, performance qualification, and soak |

Experiment 0003 is a standalone transport and native fixture suite, not the
session runtime. No target in this table has a complete qualification pass under
this ADR on current source.

CI is configuration, not evidence. `.github/workflows/runtime.yml` and
`.github/workflows/experiments.yml` select the `ubuntu-24.04` and `macos-15`
GitHub-hosted images, which are Linux x86_64 and macOS arm64; neither workflow
selects a macOS x86_64 or a Linux arm64 runner. Per-run CI results are not
retained in this repository, and running the gate on a platform is not a
qualification pass for that platform.

If a target cannot be exercised, label it unqualified and narrow the initial
release's support claim. Do not turn planned coverage into a passing result.

### Narrowed initial support claim

Until the "Still required" column is closed for a target, that target is not
claimed. No row above is closed, so **the initial release claims no platform as
supported.**

macOS arm64 and Linux x86_64 are the **release candidates**: the only targets
with any executed evidence, and the two that qualification should be run on
first. macOS x86_64 and Linux arm64 are **unqualified with no executed
evidence** — intended targets that no one has run.

ADR 0001 §8 admits no category between qualified and not, so "exercised" is a
statement about evidence and never a support claim. Documentation may describe
any of the four as intended, and must not describe any of them as supported or
tested until its row here is closed.

## Implementation milestones and gates

Apply the domain models, ports, infrastructure adapters, and conversion rules in
[ADR 0005](0005-domain-boundaries-and-adapters.md) throughout these milestones.

| Gate | Deliverable and pass criteria | Decisions to settle in that milestone |
| --- | --- | --- |
| G1: Process and bytes | Real PTY spawn, ordered input/output, bounded replay, independent observers, cancel/drain, and cleanup pass concurrent pressure tests | Shared reactor/reaper integration with the host, finite admission defaults, and exact typed lifecycle outcomes |
| G2: Native projection | Pinned Rust FFI build; ordered parsing, replies, resize, binary checkpoints, READY/history restoration, and reference-state comparisons pass | Safe native ownership and callbacks, projection budgets, and public cursor/control representation |
| G3: Default parking and replaceable storage | Built-in and injected provider contracts, storage races, failure injection, repeated restoration, bounded temporary memory, reclamation, and wake timing pass | Storage interface, default directory/quotas, runtime encryption/key lifecycle, reclaimable pool layout, and measured policy defaults |
| G4: Release qualification | Examples and event-stream adapter work; executed target matrix, required checks, performance cases, lifecycle stress, and soak are recorded | Final supported platforms, reviewed resource defaults, and any documented limitations |

G3 is required for the initial release because parking is enabled by default.
The process/byte baseline can be reviewed
at G1 and projection at G2. Bounded reader handoff needs its own platform proof
and comparative measurements after those milestones; it must not delay a correct
dedicated-reader baseline.

Use focused checks while changing each subsystem. At release qualification, run
ADR 0002's five 60-second performance repeats, 10,000 process lifecycle cycles,
100,000 attach/detach operations, and 12-hour mixed-load soak, alongside the
standard build, lint, test, documentation, and interactive checks in ADR 0001.
Revise proposed targets only with a recorded reason and review.

Save integrated results under `docs/experiments`. For each gate, record pass,
fail, or not run; source/dependency pins; toolchain and OS; limits and workload;
commands, seeds and duration; failures/timeouts; latency distributions; resource
peaks and cleanup. Preserve the earlier experiments as scoped historical
evidence. Implementation completion requires these results, not just a test plan.
