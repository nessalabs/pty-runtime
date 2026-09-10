# Full release gap audit

Audit date: 2026-09-08. Base HEAD:
`06508816021b17f325cb35dce626bef56bc15e20`, plus the current uncommitted guardian,
ordered-transfer and event-stream implementation. This is a source/assertion and
stored-evidence audit, not a test execution or release approval. Concurrent work
may supersede individual findings; record its exact manifest and proof before
changing a status. No production source was edited and no gate was run for this
audit.

Scope is **all five ADRs and every row of [requirements.md](requirements.md)**,
not the narrower guardian/transfer review milestone. Read with
[ADR 0001](../adr/0001-pty-runtime.md), [0002](../adr/0002-performance-and-stability.md),
[0003](../adr/0003-session-parking-and-state-transfer.md),
[0004](../adr/0004-integration-and-release-qualification.md), and
[0005](../adr/0005-domain-boundaries-and-adapters.md).

## Assessment

The implementation is substantial, but the full release goal is unfinished.
The existing macOS arm64/Linux x86_64 loop-3 gates prove an older, identified
implementation. The newer guardian, transfer and event adapter need consolidated
current-source execution. Most remaining release criteria also require new
harnesses, measurements or implementation; rerunning the short gate cannot close
them. No gate-wide or ADR-wide pass follows from this report.

The classifications below are:

- **Proved**: implemented and adequately proved for the row's own scoped claim,
  using linked executed evidence; this does not qualify all runtime behavior.
- **Partial**: implemented, but proof is missing, partial, historical or narrower
  than the requirement. Listed tests are candidates to reuse, not presumed passes.
- **Incomplete**: an implementation/deliverable needed by the requirement is absent
  or differs from the required behavior; further testing alone cannot close it.
- **Target missing**: required execution environment/evidence is unavailable.

A row can have several gaps; the table gives the dominant classification and
states the others. Adjacent performance/stress requirements are not silently
folded into a narrow functional row, nor are narrow tests promoted into them.

## Implementation decisions that precede full qualification

1. **READY/live-history semantics differ from ADR 0003/0004.**
   [terminal state](../../crates/infrastructure/src/terminal/state.rs) rejects
   feed/resize while history is incomplete; the application also restores all
   history before queued mutations. The actual native
   [contract test](../../crates/infrastructure/tests/terminal_contract.rs),
   `ready_fences_mutations_to_preserve_one_hundred_thousand_history_lines`,
   asserts both rejections, completes history, then feeds live bytes. This is
   deliberate protection against observed history loss, documented in
   [native README](../../scripts/native/README.md), and is preferable to silently
   losing history. It does not implement live updates at READY interleaved with
   history or report inapplicable pages as required by G2-08/G3-07. Resolve with a
   verified implementation or an explicit reviewed ADR change preserving bounded
   staging and reporting the additional wake latency. A README limitation alone
   is not recorded acceptance of changed release criteria.
2. **Crash-abandoned namespace cleanup is implemented; final qualification pending.**
   [FileCheckpointStore](../../crates/infrastructure/src/checkpoint/file.rs) now
   reclaims only unlocked namespaces in its private arena with finite entry,
   namespace and logical-byte budgets. Actual SIGKILL, live-owner exclusion,
   partial budgets and filesystem rejection tests pass on macOS; see
   [evidence](checkpoint-crash-cleanup.md). Independent review, the final frozen
   gate and Linux execution remain pending in that record. Restart restoration
   and automatic deletion of legacy directories remain outside scope.
3. **Physical reclamation remains an unresolved implementation policy.**
   [native allocator](../../scripts/native/owner.c) counts requested native bytes
   but uses malloc/free. There is no production shared reclaimable-page pool in
   this path. ADR 0003 asks for a packing/release prototype and measured tradeoff;
   earlier standalone mapping results do not qualify this allocator. Measure
   production empty/filled/compressed/parked/restored cycles and temporary peaks;
   implement/review the pool choice if malloc retention misses the objective.
4. **Missing release instrumentation and harnesses.** No runtime aggregate
   metrics API covering the ADR counters, no real-Ghostty fuzz target/corpus
   runner, no 10,000-lifecycle/100,000-attach/12-hour mixed-soak runner, and no
   interactive example were found in the audited tree. Add them. Runtime-internal
   monotonic timestamps are necessary for the specified admission→write,
   read-complete→available and admission→OS-control boundaries; public future
   round-trip timing alone measures a different interval.
5. **Guardian resource model needs explicit integration acceptance.** Two helper
   processes per live session are a material addition to the former reader plus
   shared-reaper model. Source admission bounds their multiplicity, but host
   process-limit failure, helper memory/FD/CPU, transient anchors/successors,
   retained recovery ownership and launch latency must enter the 1/32/128 sweep.
   The [guardian design review](../reviews/guardian-production-design-review.md)
   and [architecture review](../reviews/guardian-architecture.md) preserve these
   requirements. Standalone C probes are not packaged Rust release evidence.

## G1 ledger

The [raw acceptance report](g1-raw-acceptance.md) and [loop 3](loop3.md) supply
executed historical scope, including actual raw fixture assertions and source
manifests. Guardian topology changes require rerunning affected lifecycle tests.

| ID | Classification | Existing assertions/evidence and exact next work |
| --- | --- | --- |
| G1-01 | Partial | [raw_child_contract](../../tests/raw_child_contract.rs) checks child args/env/cwd/dimensions and [process_pressure](../../crates/infrastructure/tests/process_pressure.rs) rejects a symlink escape. Add sibling-prefix, changing path and invalid-input/no-child allocation checks to the public runtime matrix; document launch policy outside ADR prose. |
| G1-02 | Partial | Same child contract checks three TTY FDs, `/dev/tty`, echo absence and exact binary stdout/stderr; [raw_runtime](../../tests/raw_runtime.rs) checks synthetic input flow. Rerun controlling-session/foreground/termios assertions on packaged guardian on both current hosts; retain exact bidirectional non-UTF8 and auth-flow checks. |
| G1-03 | Proved | [raw_registration](../../tests/raw_registration.rs) plus [raw acceptance](g1-raw-acceptance.md): 16 simultaneous calls yield one launch marker and 15 duplicate errors; retained completion rejects another 16; forget permits a fresh lifetime and rejects old cursor. Loop 3 reran this. Current-source gate remains required after topology changes. Larger lifecycle stress belongs to G4. |
| G1-04 | Partial | [raw_completion](../../tests/raw_completion.rs) distinguishes real code/signal and drain, and [guardian failures](../../crates/infrastructure/tests/process_guardian_failures.rs) asserts unavailable W status after G loss. Rerun current descendant-held endpoint/final-byte/EOF-vs-truncation semantics on both OSes; exercise timeout without fabricated exit. |
| G1-05 | Proved | [raw_adversarial](../../tests/raw_adversarial.rs) actually polls waits Pending, drops them, asserts unchanged cursor and later input/output; [raw_runtime](../../tests/raw_runtime.rs) proves same process on reconnect/handle drop. Historical executed scope in loop 3 is adequate for this functional claim, not 100,000 operations. |
| G1-06 | Partial | [replay model](../../crates/domain/tests/replay_model.rs), [replay tests](../../crates/domain/src/replay_tests.rs) and raw runtime cover exact suffix/gap/cursor bounds. Add integrated multi-session global eviction with fast/stalled observers and allocation/capacity observations proving on-demand storage under pressure. |
| G1-07 | Partial | [input_reservation](../../tests/input_reservation.rs) retains global bytes/slots after caller abandonment until adapter completion; process pressure asserts partial timeout and permit reuse. Add actual short-write/EINTR/EAGAIN injection and allocation instrumentation proving no pre-admission payload copy, zeroization and no retry duplication. |
| G1-08 | Partial | Process contract/adversarial tests cover graceful/forced and dropped cancellation waits; pressure suite combines 16 short producers and a quiet cancel. It has no 60-second saturated input/query/output admitted-control p99 measurement or exact escalation-count instrumentation. Add these to full load harness. |
| G1-09 | Partial | [guardian architecture](../reviews/guardian-architecture.md) reviews live identity/group-zero signalling and fixed failure paths. [guardian failures](../../crates/infrastructure/tests/process_guardian_failures.rs) damages S/G, but does not itself inventory every fixture descendant or prove outside-session survival. Execute packaged helper foreground/background, both helper-group entrant, exit/signal barrier, owner death and escaped-session cases on both OSes. |
| G1-10 | Partial | [process_adversarial](../../crates/infrastructure/tests/process_adversarial.rs), [ddd_spawn_failure](../../tests/ddd_spawn_failure.rs), [process handles](../../tests/process_handle_resources.rs) cover selected barriers/cleanup. Add every new helper/image/channel/reader/registry acquisition edge and compare children/FDs/workers/live objects to baseline after shutdown races. |
| G1-11 | Incomplete | Dedicated readers, shared host supervisor and admission exist. Two persistent helper processes now add per-session supervision resources. Record/review this architecture adjustment and qualify full process/thread/stack/scratch/descriptor counts, quiet interruption and SIGCHLD host policy on each OS, including process-limit exhaustion. |
| G1-12 | Partial | Bounded I/O/scheduler batches and lock scopes exist; pressure tests and [scheduling adversarial](../../crates/infrastructure/tests/scheduling_adversarial.rs) cover selected wake/panic cases. Add prolonged flooder plus delayed checkpoint/sink and concurrent unrelated operations; count idle wakeups and expose actual control latency. |
| G1-13 | Partial | Domain/session admission, input reservation, raw observer limits and [projection pressure](../../crates/application/src/projection/tests/pressure.rs) assert several limits. Build concurrent at-limit/over-limit matrix for all resource classes, including retained completions, helper resources, transfer observers/slots/pins and abandoned operations. |
| G1-14 | Partial | Callback panic, scheduler panic, failed-spawn and [projection_correctness](../../tests/projection_correctness.rs) test containment and cleanup. Add complete helper acquisition fault injection and real I/O exhaustion while a separate session remains responsive; retain terminal failures and bounded retries. |
| G1-15 | Partial | Child resize acknowledgement and [projected_runtime](../../tests/projected_runtime.rs) compare real view after successful OS/model resize; fake projection process tests assert partial failure. Add deterministic real exit/OS failure/model failure interleavings and published-size/control assertions. |

## G2 ledger

The real engine tests are meaningful but not exhaustive conformance: six selected
continuation prefixes, 500-line repeated round trips, a 100,000-line oracle,
styles/modes, compression and budget failures are not all-boundary/randomized
malformed-input fuzzing. The 100,000 here counts history lines, **not attachments**.
The 10,000 loops bound decoder/compressor work, **not process lifecycle cycles**.

| ID | Classification | Existing assertions/evidence and exact next work |
| --- | --- | --- |
| G2-01 | Partial | [native build README](../../scripts/native/README.md), pinned build/verification scripts and loop-3 static native tests establish real symbols and matching source. New baseline-CPU cache fix needs current Linux/macOS execution/provenance; target-specific package builds remain separate. |
| G2-02 | Partial | [projected_runtime](../../tests/projected_runtime.rs) compares detached real-engine view against identical bytes; raw/projection admission is fixed at creation. Add long randomized detach/reconnect raw-vs-processed ledger and explicit raw-upgrade rejection assertions. |
| G2-03 | Partial | [terminal_contract](../../crates/infrastructure/tests/terminal_contract.rs) asserts selected UTF-8/styles/modes/alternate-screen and complete formatted checkpoint equality; [terminal_bounds](../../crates/infrastructure/tests/terminal_bounds.rs) checks a few malformed/budget cases. Add every byte boundary for UTF-8/CSI/OSC/DCS, seeded Unicode/VT/resize corpus, excessive parser sequences and full-history reference equality. |
| G2-04 | Partial | Exclusive native owner and callback/allocator lifetimes are documented and thread-transfer/drop tests run. Add systematic unsafe-block review record for current bridge and cancellation while FFI/decoder is active; execute available sanitizer/native memory instrumentation and record unsupported tools. |
| G2-05 | Partial | [projected_runtime](../../tests/projected_runtime.rs) child receives one query reply with two observers; [ordered_transfer_runtime](../../tests/ordered_transfer_runtime.rs) replicas discard effects; bounds test rejects reply overflow. Add real sustained queries competing with admitted user input, post-restore effect limits, explicit clipboard/desktop/image corpus and bounded failure assertions. |
| G2-06 | Partial | View contract asserts cells/styles/cursor/modes/palette/size, projected view carries byte/control labels, fake pressure tests hold/cancel pins. Add current real-engine concurrent extraction/cancellation memory peaks and cursor visibility/style edge cases; do not use formatted state as restoration payload. |
| G2-07 | Partial | Contract has UTF-8/CSI/OSC/DCS partial checkpoints, three repeated restores and compatibility errors. Bounds test only asserts overlong continuation checkpoint fails; it does not finish that sequence and prove later checkpoint eligibility/retry with unchanged full state. Add this and all split positions/repeated parking. |
| G2-08 | Incomplete | Current test intentionally rejects mutations at Usable and waits for Complete. Resolve READY/history semantics decision described above; assert history applicability/completeness and callbacks/limits after live mutation or record reviewed ADR revision plus bounded delayed-live behavior. |
| G2-09 | Partial | Compression test compares full formatted history with wide/combining text before/after bounded steps. Add unprofitable-page case, unsupported replacement engine assertion and under-load scheduling measurements; decide whether any automatic resident compression policy is warranted. |
| G2-10 | Partial | [terminal_bounds](../../crates/infrastructure/tests/terminal_bounds.rs) rejects native_bytes=1, checkpoint/view/reply/feed caps and corrupt transport. It does not sweep scrollback page overhead, aggregate live native pressure, images or malformed parser high-water allocation. Add those measurements and distinguish requested bytes from physical/allocator overhead. |
| G2-11 | Incomplete | No runnable real-Ghostty feed/resize/snapshot fuzz target/corpus/duration evidence found. Add bounded deterministic reproduction seeds and timed fuzz execution, then native instrumentation where supported. Native abort remains an owner-process failure, not recoverable Rust isolation. |

## G3 ledger

The real-clock [default parking test](../../tests/default_parking.rs) is ignored
by ordinary cargo test but has explicit [executed loop-3 evidence](loop3/default-parking/command.log).
It verifies resident at 55 seconds, parked between approximately 59 and 90 seconds
and the same live PID. That is useful threshold evidence; it does not measure
wake latency, physical reclamation or every provider failure. The fake protector
used by coordinator tests is not a substitute for real encryption integration.

| ID | Classification | Existing assertions/evidence and exact next work |
| --- | --- | --- |
| G3-01 | Partial | Default Runtime path and real default threshold executed; [checkpoint contract](../../crates/infrastructure/tests/checkpoint_contract.rs) applies same immutable round-trip/quota/delete assertions to disk and memory provider. Add exact deterministic mutation-reset threshold, raw no-parking and configured-root/default-private-store assertions on current source. |
| G3-02 | Partial | Real detached park and parked snapshot preserve process identity; one query/resize wake path runs. Add attached parking, detached sustained throughput, retained reader identity/count, parked cancel/exit and unused observer buffer reclamation measurements. |
| G3-03 | Partial | [parking tests](../../crates/application/src/projection/tests/parking.rs) inspect descriptor cursor/control, fake encode/commit barriers and stale-source deletion; transfer tests order zero-byte resizes. Add real native/store barriers directly before atomic release and forget/new-lifetime races with no stale publication. |
| G3-04 | Partial | Coordinator separates native encoding and blocking storage; tests hold commit while new feed progresses and hold pins to exhaust budget. Add real slow/failing encoder and store with aggregate temporary-memory reservations and unrelated-session control latency under load. |
| G3-05 | Partial | Fake encode/commit races preserve live owner and delete stale source; cleanup tests keep model on storage failure. Add explicit protector.protect failure/entropy failure, real disk full/short-write and cancelled pending commit integration; barriers must cover immediately-before-release mutation. |
| G3-06 | Partial | [pressure tests](../../crates/application/src/projection/tests/pressure.rs) assert all-or-none staging, cap, capacity wake, exact processed offset and one reply; fake-native panic preserves queue. Add real replay eviction during blocked encode/restore, cap-induced local read pause, exact full native reference and measured unrelated-session control progress. |
| G3-07 | Incomplete | Output coalesces restore and fake trace asserts feed after all history; current real engine cannot feed at READY. Resolve G2-08 decision, then place barriers at release/READY/history and assert exact `[0,8192)`/`[8192,8256)` coverage plus distinct timings. |
| G3-08 | Partial | Real encoded echo input causes parked wake and successful resize; fake tests order process/model partial outcomes. No public mode-dependent input encoder was found: clarify whether such an API is required or explicitly unsupported, then test every supported input mode, response wake and parked/restoring resize races. |
| G3-09 | Partial | [cleanup tests](../../crates/application/src/projection/tests/cleanup.rs) retain source/unprocessed bytes on injected restore/auth failure; native bounds reject corruption; protector tests reject altered identity. Combine real PTY/native/ciphertext corruption/wrong-build/unavailable source with capped backlog and still-working cancel/exit, proving no reset/relaunch. |
| G3-10 | Partial | [checkpoint contract](../../crates/infrastructure/tests/checkpoint_contract.rs) flips ciphertext/tag/metadata and wrong keys; [adversarial](../../crates/infrastructure/tests/checkpoint_adversarial.rs) asserts 64 nonce differences; vetted AEAD is implemented. Add real runtime injected store that asserts marker absent on every commit, real protector failure propagation and normal key/service lifetime cleanup; no owner restart promise. |
| G3-11 | Partial | Disk/memory same contract covers immutable refs, bounded reads, idempotent delete and quota; file unit hooks cover failed writes. Add same integrated PTY lifecycle outcomes for delayed/short/cancelled commit/read/delete on both providers and source pin retention through restoration. |
| G3-12 | Partial | Runtime ledgers reserve stored bytes/slots/pins/IO peaks and keep unknown failed commits charged. [Crash cleanup](checkpoint-crash-cleanup.md) now covers actual owner death, live locks, bounded partial reclamation and filesystem rejection; final review/platform gate pending. Complete the remaining concurrent overcommit and every failure/cancel permit-release matrix. |
| G3-13 | Partial | [ordered_transfer_runtime](../../tests/ordered_transfer_runtime.rs) restores two parked replicas, completes split UTF-8/CSI, applies two resizes at identical byte offset, checks one generated reply each, End boundary and equality to server. [transfer units](../../crates/application/src/projection/tests/transfer.rs) assert eviction/resync, limits/cancel/no-observer allocation. Add deterministic real attach/detach/encode/commit/output/resize barriers, many slow consumers, real journal-loss resync, repeated new checkpoint transfer and current platform gate. |
| G3-14 | Incomplete | Shared bounded scheduler exists; native malloc/free has requested-byte cap but no measured production reclaimable-pool decision. Implement/evaluate the page-packing choice and record empty/filled/compressed/parked/restored cycles, fragmentation, logical/capacity/RSS/charged/PSS/disk/peaks with separate pools and reader resources. |
| G3-15 | Partial | Fake failed store retries exactly three times and retains model; default parking timing exists. Add encode CPU/compression/cached-vs-uncached read/READY/full history/first-output measurements and long outage plateau, including failures and retained reservations. |

## G4 ledger

[Current performance test](../../tests/runtime_performance.rs) runs **one 4 MiB
trial at 1 and 16 projected sessions**. Each producer has a release input,
consumers validate every raw byte, processed cursors reach exact totals and views
show a final marker. Its replay cap is `PAYLOAD + 4096`, not the specified 1 MiB.
It measures startup, release skew, total throughput and point-sampled RSS/FDs/
descendants, not read-complete/control/input stage percentiles or peak memory.
It does not assert baseline restoration for every sampled resource. The saved
[macOS](loop3/macos-performance/records.jsonl) and
[Linux](loop3/linux-performance/records.jsonl) records are valuable scoped trials,
not any of the full performance/stability rows below.

| ID | Classification | Existing assertions/evidence and exact next work |
| --- | --- | --- |
| G4-01 | Partial | Current test proves exact bytes/processed totals for 1/16 fast producers. Build 64 resident/16 independently barrier-started producer release case at aggregate 10 MiB/s, 80×24, 1 MiB replay and recorded finite history, mixed ASCII/split Unicode/VT/color/queries. Run attached/detached/stalled observer/stalled sink/dominant producer separately. |
| G4-02 | Partial | Experiment 0003 has standalone 128 producers; current integrated test stops at 16. Execute integrated 1/32/128 marginal resource sweep, 128 independent active and mixed populations, per-producer rates/progress/fairness/blocking, and chunk/observer/grid/admission sweeps. 500 only if host permits; no global process-limit changes. Include two helper processes/session and transient helpers. |
| G4-03 | Incomplete | Runtime stage timestamps/distributions absent. Add bounded caller-owned instrumentation, warmup and at least five 60-second runs per case; input→write and read→availability p99 ≤20 ms, admitted cancel/resize→OS operation p99 ≤100 ms. Separate admission/fixture round-trip; retain p50/p95/p99/max/count/failures/timeouts. |
| G4-04 | Partial | Logical caps exist, but no 64-idle/60-second ≤1% core or ≤4 KiB control proof. Measure shared baseline, native/reader virtual stack/resident/scratch/helper/kernel/child memory separately and all queue/pin/storage caps throughout stalls. Measure sustained target throughput without loss/starvation. |
| G4-05 | Partial | Loop-3 manifests record some host/toolchain/limits and scoped trials. Add complete CPU/RAM/OS/compiler/native/build/workload/load metadata, stage copies/allocations/syscalls/wakeups/lock contention and same-host independently reviewed baseline. Compare repeated >10% throughput/p99 regressions; no self-comparison as regression proof. |
| G4-06 | Incomplete | No actual 10,000-process-cycle harness/results found. Implement seeded spawn/exit/cancel with explicit forget, launch-count and zombie checks, baseline child/FD/worker/live-object accounting and allocator-retention distinction. Decoder loops are unrelated. |
| G4-07 | Incomplete | No actual 100,000-attach/detach harness/results found. Keep workload identity and reference cursor/gap ledger, poll-and-abandon reads, release observer permits; assert resource plateau. History-line count is unrelated. |
| G4-08 | Partial | Focused tests cover selected lifecycle/pressure/panic cases. Add seeded concurrent cancel/exit, resize/exit, attach/eviction, shutdown/spawn and storage/snapshot/sink/query pressure with bounds, deadlines and surviving-session latency; run repeated cold/warm transitions. |
| G4-09 | Incomplete | No 12-hour mixed-load runner/results found. Implement continuous meaningful work and sampled resource/correctness ledger after warmup, execute full 12 hours, preserve seed/time series/failures/cleanup. Sleeping or short smoke does not qualify. |
| G4-10 | Partial | Standalone guardian experiments/probes cover owner EOF scenarios; production helper-loss test is not owner death. Execute abruptly killed real Runtime owner with foreground/background/helper-group entrants and an outside-session survivor; inventory live descendants/PTY ownership, document escape/restart limits. |
| G4-11 | Target missing | Historical actual runtime evidence exists for macOS arm64/Linux x86_64, not complete release workloads or current guardian final source. macOS x86_64/Linux arm64 lack executed qualification. Obtain hosts or explicitly narrow supported release targets; cross-build/configured CI is not execution. All claimed targets need process, codec/reclamation and workload evidence. |
| G4-12 | Partial | Real dependency adapter and [reconnect](../../tests/event_stream_reconnect.rs), [failure](../../tests/event_stream_failures.rs), [isolation](../../tests/event_stream_isolation.rs) tests assert distinct cursors, real subscription, full store, retained same-ID retry and cancelled waits. Reviews exist; collect current-source execution and full-load sink-stall resource/latency proof. No implicit persistence. |
| G4-13 | Partial | [gate.py](../../scripts/gate.py) commands cover fmt/lint/all-feature tests/raw/event-only/docs/example/helper checks and scoped performance; loop 3 ran prior code on macOS/Linux. Execute final current manifest on both hosts, feature matrix, Rustdoc/examples, native cache portability and attempted supported cross-build. A configured command is not an executed pass. |
| G4-14 | Incomplete | Facade/lockfile/notices/native instructions/Rustdoc and [event example](../../examples/event_stream_reconnect.rs) exist. README still says ordered transfer/guardian/event integration in progress and omits complete user-facing lifecycle/security/resource/platform instructions. Add interactive terminal example and real terminal smoke; concrete workspace-sdk sketch without sibling edits; update packaging/native prerequisites and exact support limitations. |
| G4-15 | Incomplete | Payload Debug implementations redact contents, but no complete aggregate metrics API found for required sessions/observers/bytes/gaps/saturation/failures/escalation/cleanup. Implement bounded caller-owned counters/timing and synthetic-secret checks across diagnostics/errors/metrics with no unbounded ID labels; document caller-held response memory. |
| G4-16 | Partial | Historical loop reports retain manifests/raw logs and honest scope. Final proof is missing; create gate-specific durable reports under docs/experiments with exact manifest/pins/OS/limits/commands/seeds/duration/failures/distributions/peaks/cleanup/review resolutions. Superseded failed Linux SIGILL must remain recorded. |

## Architecture/user-workflow ledger

| ID | Classification | Evidence and exact next work |
| --- | --- | --- |
| A-01 | Proved | Four manifests enforce std-only domain and domain+std application; [gate.py](../../scripts/gate.py) validates required package paths/edges and tests cores without native. Historical loop-3 gate passed and gate validator tests challenge forbidden dependencies. Rerun final manifest as normal release gate. |
| A-02 | Partial | Domain lifetime/replay/admission/projection/transfer rules have deterministic tests and [projection wiring review](../reviews/projection-wiring-ddd.md). New guardian and transfer changes need final current-source integrated race proof; resolve restoration semantics without moving native policy into core. |
| A-03 | Proved | Reviewed application I-prefixed ports and bounded repository contracts; hot output holds SessionContext rather than per-byte lookup. [Projection wiring](../reviews/projection-wiring-ddd.md), [event architecture](../reviews/event-stream-architecture.md) and [guardian architecture](../reviews/guardian-architecture.md) show concrete cohesive boundaries. Final source freeze/review remains a workflow condition. |
| A-04 | Partial | Native capability/compatibility errors are explicit; creation rejects missing checkpoint capability and optional compression has a typed outcome. Add replacement terminal factory contract/admission tests proving no process allocation on unsupported required behavior and incompatible existing-state replacement; resolve mutation-during-restore capability versus required ADR behavior. |
| A-05 | Partial | Terminal, OS, checkpoint and event conversions stay in infrastructure; native/event contract tests assert selected invalid values and redaction. Complete current API audit and invalid enum/required-field/size matrix, including new guardian protocol and synthetic secrets. |
| A-06 | Partial | Platform watches/readiness and scheduling remain behind common ports. Historical Linux/macOS process contracts run; new helper topology and adapter overhead need actual current per-platform resource/load tests. PSS/RSS/charged/virtual counters must retain separate meanings. |
| A-07 | Partial | Many separate correctness/organization/DDD reports exist under docs/archive/reviews, including resolved coordinator and guardian findings. Finish current guardian independent correctness, final integration/organization review and linkage of findings to exact final source; no report title alone proves coverage. |
| A-08 | Proved | [coding_standards.md](../../coding_standards.md), mechanical gate, validator tests and historical executed logs enforce formatting/lint/tests/dependency/file size. Gate explicitly does not substitute for specialist review. Full release runs must preserve this workflow. |
| A-09 | Partial | Existing repository/history provides completed loop commits, but this audit did not reverify private visibility/remote from authoritative host. Current HEAD has uncommitted new production/review work. Root must verify privacy, commit/push reviewed source/docs/evidence to main, record clean tracked state and actual CI outcome after final gate. |

## Execution order for the remaining work

1. Resolve READY/live history implementation, bounded abandoned-file cleanup,
   native memory/reclamation policy and guardian resource contract. Add missing
   metrics, fuzz runner, interactive example and documentation. Keep deterministic
   regressions for all fixes before long runs.
2. Add a reusable acceptance harness that selects cases/seeds/duration/repetitions,
   uses independent per-PTY producers/barriers and validates byte/control/gap state
   during the load. Separate monotonic runtime stage timing from fixture latency.
   Log bounded periodic resource/counter snapshots and explicit failures; never
   remove timeouts from distributions. Add lifecycle and observer stress modes.
3. Freeze current source, run complete mechanical and focused native/process/
   provider/transfer/guardian fault suites on macOS arm64 and Linux x86_64, then
   execute required five 60-second workloads, resource sweeps and exact cycle
   counts. Current guardian means old process-resource numbers need replacement.
4. Start full 12-hour mixed soak only once deterministic checks pass. Preserve
   evidence through completion. Obtain missing targets or record a reviewed,
   explicit support restriction; do not imply four-target release qualification.
5. Review thresholds, baseline regressions, memory pool choice and any deviations
   before updating the completion ledger. Publish gate-specific pass/fail/not-run
   evidence and final main/CI state. Dynamic reader handoff remains explicitly
   deferred; it neither blocks this release nor counts as a passed requirement.
