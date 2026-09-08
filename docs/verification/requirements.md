# ADR completion requirements and adversarial proof ledger

Audited 2026-09-08 against ADRs 0001–0005. This ledger preserves the full proposed
release contract. **Every integrated requirement below is pending** until a
linked, executed result proves it. A test name, implementation, configured CI job,
or passing standalone fixture is insufficient evidence of an integrated gate.
Updating this ledger requires recording the exact source revision and evidence;
do not infer completion from neighboring rows. The runtime was not present at
this audit's initial inspection; later implementation requires fresh review.

Sources: [ADR 0001](../adr/0001-pty-runtime.md),
[ADR 0002](../adr/0002-performance-and-stability.md),
[ADR 0003](../adr/0003-session-parking-and-state-transfer.md),
[ADR 0004](../adr/0004-integration-and-release-qualification.md), and
[ADR 0005](../adr/0005-domain-boundaries-and-adapters.md).

## Evidence rules and existing scope

`pending` means completion is unproven; `fixture-only` means evidence establishes
only the explicitly described fixture claim. Use `pass`, `fail`, or `not run`
for each executed gate report and retain failures/timeouts. No blanket ADR pass
is valid while any required row lacks adequate proof. Dynamic reader handoff is
an explicitly deferred extension, not an initial dedicated-reader release gate.
500-session capacity is conditional on available host capacity and must never be
claimed without execution; 128 independently active sessions is required.

The existing [Experiment 0003](../experiments/0003-cross-platform-concurrent-workloads.md)
raw [macOS records](../experiments/data/0003/macos-results.jsonl) and
[Linux records](../experiments/data/0003/linux-results.jsonl) were revalidated on
2026-09-08 using `experiments/gate.py`'s `summarize(metadata, records)` with each
host's metadata: each had 245 valid records, 49 cases, five repetitions. This
proves recorded standalone PTY/native fixture validation, not a new execution
of those workloads. Native and PTY fixtures are separate processes; production
Rust/Ghostty integration, encryption, storage-provider contracts, lifecycle,
control latency, and soak remain pending. The 19 validator tests described by
the experiment report validate the evidence validator, not the runtime.

| Existing evidence | Verified scope | Cannot prove |
| --- | --- | --- |
| Experiment 0003, macOS arm64 / Linux x86_64 | 128 independent producers; fixed dedicated/shared readers; byte/checksum and fixture cleanup checks; scoped latency/memory comparison | 128 projected sessions, runtime cancellation latency, production reader stack safety, 60-second repeated integrated performance |
| Experiments 0002/0003, pinned native C fixtures | Native encoding, continuation, restore, compression, memory/reclamation and synthetic corruption checks | Rust FFI ownership, default encrypted parking, races, allocator production suitability, complete terminal conformance |
| Experiment baseline self-comparison | Validator/gate self-consistency | Regression freedom against an independent implementation revision |
| Configured platform workflow | Intended CI coverage | Executed target qualification |

The [review-loop 2 record](loop2.md) adds executed raw-runtime and concrete-adapter
tests. It is a partial implementation checkpoint and does not discharge a whole
integrated milestone. Every row below still requires its full scoped proof.
The [review-loop 3 record](loop3.md) adds integrated projection, encryption/parking,
restoration, bounded snapshot pins, default 60-second parking evidence, scoped
performance, cleanup regressions and independent architecture review. It does not
yet establish ordered observer continuation or full release qualification.

## G1: identity, processes, bytes, and portable boundaries

All rows pending. Source sections are given as ADR number and section name.

| ID | Requirement / source | Proof required |
| --- | --- | --- |
| G1-01 | Absolute executable/cwd, literal args; canonical cwd inside supplied roots; empty env default, inheritance/removals/overrides; validated finite dimensions (0001 Commands, Secrets) | Real child fixtures report args/env/cwd/dimensions; symlink escapes and sibling-prefix paths rejected; invalid inputs allocate no process; document path policy is not a sandbox |
| G1-02 | Real controlling PTY/session; stdout/stderr merged as bytes; initial echo disabled (0001 Process) | `isatty`, session/foreground-group and termios assertions; non-UTF8 bidirectional payload; synthetic authentication prompt/code/success and actual exit; submitted input absent with echo off |
| G1-03 | Atomic register-once ID, fresh lifetime; duplicate never respawns; lookup reconnect; explicit removal only when finished (0001 Identity, 0005 Repository) | Concurrent duplicate spawn barrier; fixture launch counter/PID; completed retained ID rejects spawn until removed; new lifetime rejects old cursors |
| G1-04 | Independent process, drain, projection, observer states; actual exit separate from timeout/supervision failure (0004 State) | Zero/nonzero/signal exit; descendant-held endpoint; final bytes after child exit; explicit EOF or truncated drain outcome; wait timeout never fabricates exit |
| G1-05 | Detach, handle drop and cancelled read do not kill/relaunch or advance cursor (0001 Ownership/validation) | Same child identity across many observers/reconnect; cancelled read resumes at unchanged position; no observer-owned process cleanup |
| G1-06 | Bounded on-demand replay and shared retention; exact gaps and bounded pages, foreign/future cursor errors (0001 I/O, 0002 Resources) | Reference byte ledger under per-session/global eviction; multiple fast/stalled observers; no full cap preallocation at spawn; gap precedes retained suffix, correct half-open ranges |
| G1-07 | Input admission before copying; chunk/bytes/slots/operations/waiters bounded; transient input cleared; partial writes explicit (0001 I/O/Secrets) | Saturation and allocation instrumentation; short-write/EINTR/EAGAIN and abandoned wait; no implicit resend or silently dropped admitted bytes; synthetic marker redaction and no persistence |
| G1-08 | Cancellation admission survives dropped waiter, coalesces escalation once; reserved control path (0001 Ownership, 0004 Cancellation) | Cooperative and SIGTERM-ignoring fixtures; many cancel callers abandon waits; full input queue/output flood; actual shared exit result and bounded OS-control latency |
| G1-09 | Synchronized signalling and reaping prevents PID reuse; session and foreground process groups covered (0001 Ownership) | Deterministic exit-versus-signal interleavings and identity protocol review; foreground job-control descendant tests; escaped-group/session limits explicitly tested/documented |
| G1-10 | Graceful shutdown rejects spawn, bounded grace/escalation; runtime drop/worker abortion cleanup (0001 Ownership) | Spawn/shutdown barrier at every acquisition; active/idle/blocked-input fixtures; actual child reaping and descriptors/registrations/workers/objects baseline restoration |
| G1-11 | One interruptible dedicated reader per live PTY, bounded admitted count/stack/scratch; shared reaper, no waiter/timer per session (0001–0002, 0005 Platform) | Quiet blocked readers wake on shutdown without closing surviving session PTYs; thread/stack/descriptor accounting; stack safety under native/control load; race-safe host SIGCHLD integration |
| G1-12 | Finite work batches; no global lock during I/O/native/snapshot/publication; unrelated sessions progress (0002 Scheduling) | Flooder/slow-consumer test with concurrent input/resize/cancel; lock-scope review; short I/O/readiness clearing; no busy idle polls or lost wakeups |
| G1-13 | Typed admission errors for global/per-session registry, observers, reads, operations, IDs/metadata/workers (0001 Identity, 0002 Resources) | Boundary-at-limit and concurrent admission tests before allocation; retained completed sessions charged; abandoned observers/operations release permits |
| G1-14 | Recoverable task/I/O failures confined; no automatic relaunch or infinite retry (0002 Stability) | Failure injection at each partial-spawn/acquisition/worker phase; surviving sessions continue; stable explicit terminal failure outcomes |
| G1-15 | Resize OS/model outcome truthful, ordered, invalid values rejected (0001 Projection, 0004 Races) | Child size acknowledgement, snapshot comparison after G2; OS and model partial failures plus exit races; published size tracks actual result, no false atomic-success claim |

## G2: real terminal engine and FFI

All rows pending; native fixture-only evidence above does not discharge them.

| ID | Requirement / source | Proof required |
| --- | --- | --- |
| G2-01 | Actual pinned static `libghostty-vt` build, matching generated Rust bindings, Zig 0.16.0 / Ghostty `82232ecde55405559dec29c5466cb9e39938cb41` (0001 Dependency/G2) | Reproducible source/hash/toolchain/build log, linked real symbols; exercised allocation/feed/query/resize/destruction; concrete build failures retained, no substitute backend |
| G2-02 | Explicit raw versus projection at creation; detached output feeds same authoritative terminal exactly once and in order (0001 Projection) | Uninterrupted real-engine reference receives identical chunks/controls; reconnect uses existing model; raw truncated replay cannot silently upgrade into a complete terminal |
| G2-03 | Split UTF-8, combining/wide text, CSI/OSC/DCS, malformed/excessive input, alternate screen, styles/modes preserved (0001 Validation, 0002 Stability, 0004 Parking) | Deterministic all-boundary and seeded randomized sequences against real wrapper/reference; bounded parser behavior and final views; arbitrary malformed streams cannot bypass limits |
| G2-04 | Safe exclusive native ownership; synchronous bounded nonreentrant callbacks; no use-after-free during cancellation (0002 Scheduling, 0004 Parking) | Safety review for each unsafe/FFI block, callback userdata/allocator lifetime proof; injected cancellation while FFI active; available native memory/error instrumentation with limitations recorded |
| G2-05 | Generated replies use same ordered writer as user input; bounded responses; clipboard/desktop/image features disabled initially (0001 Projection, 0003 Ordered output) | Real query child gets exactly one correct response with multiple observers; flood overload bounded; no callback write reentrancy; effect policy and image resource limits verified |
| G2-06 | Views expose dimensions/cursor visibility/active screen/modes/text/styles and processed cursor (0001 Projection, 0005 Terminal) | Cursor/control-labelled snapshots agree with reference; finite request count/output bytes; cancelled extraction releases permit; formatted views never treated as restore payload |
| G2-07 | Binary checkpoints preserve partial continuation and exact engine identity; bounded tracking enabled before feed and after restore (0003 Native) | Split unfinished UTF-8/CSI/OSC/DCS round trips and repeat parks; continuation-limit exhaustion keeps resident state and retries valid boundary without loss |
| G2-08 | Distinct usable-state/complete-history progress; limits and callbacks rebound before live feed (0003 Restore, 0005 Terminal) | READY then bounded history steps interleaved with live operations; inapplicable old pages recorded; no premature complete-history claim; replies and limits still enforced after restore |
| G2-09 | Incremental resident compression serialized, optional capability explicit, bounded work outside hot I/O (0002 Parking, 0005 Terminal) | Reference equality before/after compression, unprofitable pages, scheduling latency; replacement engine unsupported capability typed; compression not required immediately before identical-size parking |
| G2-10 | Scrollback/cells/graphics/parser/snapshot budgets correctly scoped (0001 Projection, 0002 Resources) | Limits and pressure tests on native allocation; page overhead distinct from logical scrollback; raw/empty/filled/projected memory separately measured |
| G2-11 | Fuzz bounded feed/resize/snapshot sequences with real Ghostty (0002 Stability) | Runnable fuzz target, seed/corpus and executed duration/iterations, sanitizer/instrumentation output where supported; reproduce all failures; document native abort is not in-process crash isolation |

## G3: parking, encryption, storage and ordered transfer

All rows pending. G3 is mandatory for the initial release; disabling parking does
not satisfy it. The default eligibility begins after 60 seconds without output
or model mutation; any change needs recorded rationale and review.

| ID | Requirement / source | Proof required |
| --- | --- | --- |
| G3-01 | Default automatic parking, built-in private temp namespace, configurable root/limits, injectable same-contract provider (0003 Store) | Default construction with no custom store; raw sessions skip native parking; deterministic clock threshold; real disk and injected provider contract suite |
| G3-02 | Independent model/reader/observer policies; no observer does not imply low throughput; quiet reader/readiness/reaping retained (0001 Idle, 0003 Decision) | Park while detached/attached; output wakes immediately without periodic minute polling; reader identity/count retained; unused buffers released separately; cold exit/cancel works |
| G3-03 | Checkpoint exact processed byte cursor + ordered control generation + lifetime + attempt generation (0003 Parking, 0004 Races) | Barriers at encode/commit/release prove byte/control positions; resize with zero output still orders correctly; stale completion cannot replace newer state or revive forgotten session |
| G3-04 | Encode exclusive but storage outside model/global lock; bounded immutable staging; fail callback promptly (0003 Parking) | Slow/failed encoder consumer never blocks in native callback; peak reservations/pins/commits accounted; concurrent output staged; other sessions and OS control remain responsive |
| G3-05 | Output/mutation before commit invalidates park; complete verified storage before model release; failure preserves original (0003 Parking, 0004 Races) | Every commit interleaving, including mutation immediately before atomic release decision; unused checkpoint deletion; encoding/encryption/full-disk/cancelled-commit failures retain model |
| G3-06 | Lossless parser staging separate from evictable replay; finite per-session/global caps and local backpressure (0002 Resources, 0003 Ordered output) | Replay eviction during encoding/restoration cannot lose parser bytes; backlog cap pauses affected reads; unrelated sessions/control/reaper progress; stalls measured explicitly |
| G3-07 | Post-release output coalesces restore; READY precedes queued live bytes exactly once (0003 Restore, 0004 Races) | Barrier directly after release and at each history step; reference bytes `[0,8192)` + `[8192,8256)` fed once; first-byte/READY/history latencies separate |
| G3-08 | Encoded input works parked; model-dependent input restores; parked/restoring resize ordered and partial errors truthful (0003 Decision, 0004 Races) | Separate input modes with response-triggered wake; resize/output/history races against native reference |
| G3-09 | Restore failure explicit, child/raw I/O/recovery source preserved; no empty reset/relaunch/parser drop (0003 Restore, 0004 State) | Corrupt/truncated/wrong-engine/wrong-native-build/unavailable/authentication-failed checkpoint; bounded backlog then explicit backpressure; cancel/exit still work |
| G3-10 | Vetted authenticated encryption before store, authenticated descriptor metadata, per-owner keys (0003 Store, 0005 Protector) | Injected provider asserts no plaintext marker; ciphertext/header/tags/identity tampering rejected; unique nonce/key use reviewed; redacted errors; normal key cleanup and no owner-restart promise |
| G3-11 | Immutable IDs, all-or-error commit, bounded reads, typed errors, cancellation, no silent eviction of sole copy (0003 Store) | Real disk/injected delays, short reads/writes, full quota, cancelled commits, deletion races; successful complete readable source remains pinned until release; provider replacement same observable outcomes |
| G3-12 | Per-session/global encrypted bytes, pending commits, pins, restore/encode peaks, abandoned data bounded (0002 Resources, 0003 Store) | Concurrent reservations cannot overcommit; failure/cancel releases each permit once; source retained through restore; namespace-scoped bounded crash cleanup and private permissions verified |
| G3-13 | Compatible checkpoint + original bytes + ordered controls observer transfer, explicit resync on lost retained range (0003 Ordered output) | Snapshot-to-live attach/detach/output/resize barriers; consumer reference model equality; no duplicate replies; parked immutable snapshot attach does not need server restoration |
| G3-14 | Reclaimable storage/pools and bounded shared expiry mechanism; physical reclamation measured (0002–0003 Parking) | Empty/filled/compressed/parked/restored allocations, capacities, RSS/charged/PSS, temporary peaks, disk bytes; repeated cycles and fragmentation; no one timer per chunk/thread per PTY; platform-specific pool tradeoff reviewed |
| G3-15 | Native/disk wake metrics distinct, bounded retries retain terminal on failure (0002 Parking, 0003 Qualification) | Encode CPU, compression, cached/uncached I/O, READY, full history, first output separately; repeated storage outage bounded metadata/work; no state loss to meet memory target |

## G4: release workload, platform execution, and deliverables

All rows pending. Every numeric threshold is a proposed acceptance criterion,
not an existing measurement. Changes require a documented reason and review
before declaring completion, rather than silently changing the fixture to pass.

| ID | Requirement / source | Exact proof required |
| --- | --- | --- |
| G4-01 | Controlled integrated load (0002 Targets, 0004 Pressure) | Release build, 64 resident sessions / 16 independent barrier-started producers, aggregate 10 MiB/s, 80×24, 1 MiB replay cap each, finite recorded Ghostty scrollback; ASCII/split UTF-8/VT/color/queries; attached, detached, stalled observer/sink, and one dominant producer |
| G4-02 | Active/idle/mixed capacity beyond controlled reference (0002–0004) | 1/32/128 marginal resource sweep; 128 independently active producers and mixed 128 sessions; per-producer rate sweep, fairness/blocking/progress; session/rate/chunk/observer/grid sweep up to typed overload; 500 active/idle only on sufficient host, no global limit edits |
| G4-03 | Timing methodology and thresholds (0002 Targets) | At least five post-warmup 60-second runs per throughput/latency case; monotonic admitted-input→PTY-write p99 ≤20 ms while writable, read-complete→observer-available including ordered feed p99 ≤20 ms, admitted cancel/resize→OS-operation p99 ≤100 ms; separate admission and fixture round-trip; p50/p95/p99/max/count/timeouts including failures |
| G4-04 | Idle CPU/memory and bounded saturation (0002 Targets) | 64 idle fixtures for 60 seconds, owner ≤1% of one core; idle control ≤4 KiB/session plus measured shared baseline; report dedicated stacks virtual/resident/scratch separately; queue/replay/staging/snapshot/storage/event caps throughout stalls; sustained 10 MiB/s without unexpected loss/starvation |
| G4-05 | Host baseline and regression review (0002 Targets) | Record CPU/RAM/OS/Rust/Zig/native pins/build flags/grid/all limits/competing load; stage throughput/CPU/memory/copies/allocations/syscalls/wakeups/lock contention; reviewed same-host baseline; repeatable >10% throughput or p99 regression fails pending explanation/acceptance |
| G4-06 | 10,000 spawn/exit/cancel cycles (0002 Stability) | Actual executed count and seeds, no duplicates/zombies/descriptor or metadata growth after explicit forgetting; baseline child/descriptor/worker/live-object counts restored; allocator retention separately reported |
| G4-07 | 100,000 attach/detach operations (0002 Stability) | Actual count, stable identity, exact cursor/gap accounting, abandoned-observer cleanup |
| G4-08 | Concurrent lifecycle and pressure stability (0002 Stability) | Seeded cancel/exit, resize/exit, attach/eviction, shutdown/spawn; stopped-input, continuous-output/query floods, failed-spawn/resource-exhaustion, snapshot/sink pressure, repeated cold/warm; bounded isolation and control latency |
| G4-09 | 12-hour mixed-load soak (0002 Stability) | Full 12 hours executed after defined warmup, deterministic seeds and workload, resource time series/plateaus, correct byte/gap ledger, no accumulating children/descriptors/workers or unexpected task failure; short smoke is not substitute |
| G4-10 | Abrupt owner termination (0001 Ownership, 0002 Stability) | Real owner-death fixture and OS descendant behavior; document escaped descendants and no process recovery from snapshots |
| G4-11 | Platform qualification (0001 Targets, 0004 Platform) | Execute native and real runtime tests for claimed targets: macOS arm64/x86_64 and Linux arm64/x86_64 tracked individually; controlling terminal/foreground groups/readiness/partial I/O/hangup/reaping/resize/cancel/cleanup/codec/reclamation; unexecuted target explicitly unqualified |
| G4-12 | Real event-stream adapter (0001 Event-stream/Validation) | Pin `66ba7525260040d6265276692088dca6dae0737e`; actual publication/subscription/replay-to-live reconnect; typed output/gap/completion; distinct event versus byte cursor; sink-full/stall/rejection/cancel/retry isolation and bounds; no implicit persistence |
| G4-13 | Required build/check commands (0001 Validation) | Execute `cargo fmt --check`; `cargo clippy --locked --all-targets --all-features -- -D warnings`; `cargo test --locked --all-targets --all-features`; build/test with event-stream disabled; Rustdoc warnings denied; examples compile; Linux CI actually executed and local cross-build attempted where toolchain permits |
| G4-14 | Package/docs/examples (0001 Deliverables/Integration) | Typed Rust facade plus Cargo.lock; pinned build prerequisites and third-party notices; Rustdoc; README ownership/replay/security/resources/platform limits; interactive real-terminal smoke and event-stream/reconnect example; workspace-sdk integration sketch without modifying sibling package |
| G4-15 | Redacted bounded diagnostics and caller-owned metrics (0001 Secrets, 0002 Diagnostics) | Counters sessions/observers/admitted/retained/gaps/saturation/read/write/failures/escalation/cleanup; synthetic secrets absent from Debug/errors/logs/metrics; no unbounded ID labels, no mandatory cloud/logger/UI; caller-retained response memory distinction documented |
| G4-16 | Durable proof after every gate (0004 Evidence) | Gate-specific report under docs/experiments: pass/fail/not run, exact source/dependency pins, toolchain/OS, workload/limits, runnable commands/seeds/duration, raw failures/timeouts, distributions, resource peaks/cleanup, review findings and resolutions; do not label configured/unrun checks passing |

## Architecture and adversarial review gates (ADR 0005)

These apply throughout G1–G4, not only at final release. All pending.

| ID | Requirement | Proof required |
| --- | --- | --- |
| A-01 | Internal domain/application/infrastructure crates plus consumer facade; inward dependencies enforced | Cargo metadata gate rejects forbidden/transitive edges; domain/application build and test without OS reactor/native/executor; domain std only, application domain+std only |
| A-02 | Domain aggregate owns lifetime/cursor/admission/state/generation rules; separate process/drain/residency/observer states | Deterministic transition/race tests; application coordinates translated outcomes; no native descriptor/handle/error codes or DTO-as-business-model in domain |
| A-03 | Application I-prefixed boundary ports, bounded atomic repositories; no repository lookup per byte | `IProcessBackend`/`IProcessSession`, `ITerminalFactory`/`ITerminal`, `ISessionRepository`, `ICheckpointStore`, `ICheckpointProtector`, `IEventPublisher`, `IClock` contracts reviewed for ownership/atomicity/bounds/cancellation; no interface-for-every-struct proliferation |
| A-04 | Engine-neutral public/core models; replacement validation and compatibility truthful | Real and deterministic engine contract suite; unsupported required parking rejected; optional compression typed; opaque format identity; foreign engine existing-session change rejected without explicit verified conversion |
| A-05 | External DTO/serialization/error conversions in infrastructure only | API and source review plus conversion tests for invalid sizes/enums/required fields/compatibility; redacted stable core error types; no C cells/pointers/Ghostty enums/OS errno/event-store cursor leaks |
| A-06 | Platform adapters and scheduling behind common ports | Separate kqueue/macOS and epoll/Linux mechanics; same process contract executed per platform; physical counters stay semantically distinct; adapter overhead included in G4 |
| A-07 | Focused modules/public exports/Rustdoc; auditable patterns and ownership | Dedicated adversarial reviews for correctness, code organization/file bloat, DDD/dependency adherence, architecture/design patterns; findings linked to fixes or justified disposition; review after each loop |
| A-08 | Always-run coding standards gate (user requirement) | `coding_standards.md` defines enforceable scope; check script/CI runs formatting/lint/tests/dependency/file-organization rules appropriate to loop; evidence records command and exit; document reviewer checks automation cannot establish |
| A-09 | Private NessaLabs package repository and main review loops (user requirement) | Git initialization and private `nessalabs/pty-runtime` remote verified from authoritative host; source/docs/evidence committed and pushed to main for completed loops, clean tracked state and CI outcome recorded |

## Deferred handoff extension

These requirements are **deferred, not passed**. Dedicated readers remain the
initial default for both active and quiet sessions. If dynamic handoff is later
enabled, ADR 0003 requires a separate platform experiment proving: old owner
acknowledges stop and publishes all read bytes before new ownership; exactly one
reader owns each descriptor; generation rejects stale readiness; new owner checks
pending data immediately; silent read interruption without closing PTY; writer
behavior survives blocking-mode changes; output/input/cancel/exit races lose no
bytes/wakeups; globally bounded workers, hysteresis and thread churn. Compare
mixed-population throughput, fairness, wake latency, handoff cost and memory at
equal grid/history/output/observer settings before enabling the policy.

## Initial adversarial priorities

1. Establish identity and resource reservations before OS spawn. The dangerous
   failure is a child launched after shutdown or left alive when registration,
   reader creation or supervisor attachment fails. Test every acquisition edge.
2. Choose and prove the shared reaper/host SIGCHLD mechanism before relying on
   signals. A mutex around a stored PID alone does not prove it cannot be reused
   after another reaper consumes the child. Account for foreground groups too.
3. Separate lifecycle control from congestible input/native work. Interruptible
   quiet readers, abandoned cancellation waits and inherited endpoint drain are
   mandatory early tests; happy-path output does not exercise them.
4. Keep original byte positions and ordered controls distinct from terminal
   processed position. Reserving parser input separately from replay is necessary
   before asynchronous encode/storage can safely coexist with ongoing output.
5. Treat native handles, callback userdata and allocator buffers as one lifetime
   protocol. Cancellation cannot destroy them while synchronous FFI is running;
   callbacks cannot synchronously recurse into writer/model operations.
6. Build generation-checked parking as a transaction with retained source/model
   on every failed edge. Encryption success alone is not commit; commit success
   alone is not permission to free a model mutated while storage was slow.
7. Start long-run qualification only on an implementation that already passes
   deterministic failure/race tests. Preserve the full 12-hour requirement and
   produce sampled resource/accounting evidence, not merely a sleeping process.
