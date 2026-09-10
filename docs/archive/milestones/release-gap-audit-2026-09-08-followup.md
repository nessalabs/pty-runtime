# Release gap audit follow-up — 2026-09-08

**Release qualification remains incomplete.** This is a read-only product-code
follow-up to [the historical audit](release-gap-audit.md), covering all 67 rows in
[the full requirements ledger](requirements.md). Historical reports and failures
remain intact. No crash reproduction or malformed-input investigation was
performed for this audit.

`proved` means the row's stated bounded contract is backed by executed evidence
for the explicitly named source/platform; it is not whole-release acceptance.
`partial` means meaningful implementation/execution exists but part of the row's
required proof is missing. `incomplete` denotes an unmet mandatory deliverable,
known blocker, or failed global target. `not-run` means the required qualification
workload was not executed to its required scope; a runner or narrower fixture may
exist. A failed workload is identified as failed, never silently called not-run.
Every row preserves source/platform limits and exact next proof. The classifications
are 16 proved, 44 partial, 4 incomplete and 3 not-run; these are scoped evidence
judgments, not a percentage of release readiness.

## Source and evidence cutoff

- Loop4 macOS arm64 attempt3 and Linux x86_64 attempt2 both passed with identical
  source inventories and unchanged source: 93.35s and 260.03s. Their Rust compilers
  differ (1.98.1 versus 1.97.1); source equality does not mean identical toolchains.
- The same earlier frozen source produced the release image for actual 10,000
  lifecycle/100,000 attach counts and 256 race rounds on macOS. Those counts are
  complete; their 65.8s / 3.75s durations do not establish a memory plateau or soak.
- Latest macOS candidate2 gate was independently read after completion:
  `passed=true`, `exit_code=0`, `sources_unchanged=true`, 95.58s, 281 source files.
  [Its metadata][M2 metadata] and [full log][M2 log] cover the later bitmap guard,
  application fault tests, native boundary gate and load harness regressions.
  Latest candidate2 Linux execution was not yet present at cutoff. The earlier
  matching Mac/Linux pair must not be relabeled as this latest pair.
- Full attached load failed overall: two of five required trials passed. Subsequent
  deterministic harness fixes are now gated, but no corrected full five-repeat
  result is assumed. All original failures remain retained.
- READY/live history with explicit skipped pages, bounded checkpoint crash cleanup,
  caller-owned diagnostics, ordered/event transfer, interactive example and owner
  death harnesses now exist and have scoped execution. Their absence in the older
  audit is superseded; their full qualification is not inferred.
- Seed 201 remains a known unresolved native process crash and its investigation is
  paused. Earlier native regression fixes and the separately confirmed bitmap
  allocator guard do not prove it resolved. No complete native corpus is claimed.
- Current source inspection supplements the executed frozen results; ongoing
  working-tree edits are not assumed to share those results. This report does
  not freeze product code. Source snapshots and evidence hashes are in
  [the audit inventory](release-gap-audit-2026-09-08/source.json). HEAD 52ad04c is a
  base revision only; later candidate identity is its full file inventory.

## G1: identity, processes, bytes and portable boundaries

| ID | Status | Executed evidence / current implementation | Remaining proof or scope limit |
| --- | --- | --- | --- |
| G1-01 | proved | Real-child literal args, exact Empty environment, canonical roots, dimensions, binary stdout/stderr and invalid-command tests pass in [M2 log] and [L log]; see `explicit_environment_literal_args_canonical_cwd_and_ordered_resize_reach_real_child`, `literal_arguments_empty_environment_overrides_and_canonical_roots`, and [command contracts]. [Usage] explicitly limits path validation. | Proved for the recorded macOS candidate and earlier matching Linux source, not a filesystem sandbox or qualification of unexecuted targets. Later changes require the same contracts again. |
| G1-02 | proved | `controlling_terminal_echo_off_ordered_input_and_real_exit`, real-child contract tests and binary-byte observer assertions pass in [M2 log]/[L log]. [Interactive] adds literal/Unicode/ANSI, actual status and controlling-host-terminal smoke on both hosts. | The bounded fixtures establish the listed controlling-PTY and echo/byte contracts; arbitrary interactive applications and full-load timing remain G4 obligations. |
| G1-03 | proved | [M2 log]/[L log] record duplicate-spawn barriers, retained completed-ID behavior, fresh-lifetime/foreign-cursor tests and failed-spawn lookup settlement; see [raw contracts] and the atomic repository/domain identity tests. | Proved for the executed registration/lifetime contract. This does not confer authority to reuse IDs without explicit finished removal or establish a restart-persistent repository. |
| G1-04 | proved | `signal_death_and_descendant_drain_never_become_fabricated_success`, `descendant_endpoint_has_bounded_drain_separate_from_exit`, helper-loss and immutable-End tests pass in [M2 log]/[L log]. [Pressure fix] retains Linux reset/EOF red/green and repeated green evidence. | Actual exit, supervision error, parser failure and drain completion stay distinct. Qualification of every injected host failure and every target remains partial under G1-14/G4-11. |
| G1-05 | proved | `detach_and_cancelled_observation_preserve_live_process_and_cursor`, polled-wait cancellation and same-lifetime lookup tests pass in [M2 log]/[L log]. [Repetition] records 100,000 attach/drop operations with unchanged PID/lifetime/cursor. | The long attach count used a quiet retained child; active gap churn is covered separately by bounded tests, not 100,000 active-output iterations. |
| G1-06 | partial | Domain replay and facade gap/retention tests, exact binary suffix and half-open ranges, foreign/future cursors, fixed pages, and global admission tests pass in [M2 log]/[L log]; [raw contracts] and [replay source]. | Complete multi-session fast/stalled observer sweeps with measured retained capacity and fairness at release load are absent. No full configured replay cap is allocated at raw spawn, but integrated allocation/peak proof across the required matrix remains incomplete. |
| G1-07 | partial | [Input reservations] and `global_bytes_and_slots_survive_abandoned_wait_and_release_on_adapter_completion` prove adapter-owned lease lifetime; [M2 log]/[L log] cover full input, short/partial outcomes, generated reply no-resend and synthetic redaction. | Full deterministic EINTR/EAGAIN/short-write acquisition matrix, allocation-before-copy instrumentation across all admission edges, and release-load input latency are not discharged by selected unit/PTY cases. |
| G1-08 | partial | `cancellation_bypasses_full_input_and_escalates_once`, distinct-foreground dispatch acknowledgements, dropped waiter and interactive stalled-child cancellation pass in [M2 log]/[L log], [Diagnostics review] and [Interactive]. | Five successful full-load repeats with bounded cancellation dispatch and the complete abandoned-caller/output-flood matrix remain absent; two successful load trials do not qualify latency. |
| G1-09 | partial | [Guardian review], foreground migration tests and direct-child/group authority review accompany actual macOS/Linux contracts. [Owner death] verifies foreground/background/helper-group cleanup plus intentional escape/outside survival 25 times per host. | Exhaustive PID/reaping/host-policy/acquisition interleavings and resource/discovery faults remain open in guardian design review. Escaped sessions are explicitly outside the cleanup promise; this limitation must remain in release documentation. |
| G1-10 | partial | Spawn/shutdown barriers, blocked-reader/output, immediate drop, destructor/callback panic, lost completion cleanup and [Repetition] baseline FD/child/reservation checks pass; [M2 log], [L log], [Shutdown evidence]. | Not every OS acquisition/thread/registration failure has deterministic rollback injection. Short repetition proves observed quiescent cleanup, not long-term growth freedom or every worker-abortion path. |
| G1-11 | partial | Dedicated readers and shared supervision/scheduling are implemented; quiet-reader shutdown, worker-bound rejection, reentrant callbacks and host SIGCHLD-policy tests pass [M2 log]/[L log]. [Guardian design] records topology and resource limits. | Production reader virtual/resident stack and scratch accounting, stack safety under combined native/control load, and complete per-target host signal integration remain unqualified. The two-helper-per-session topology must be included in capacity measurements. |
| G1-12 | partial | Lock-scope reviews, bounded scheduler tests, blocked spawn isolation, reader-failure survival and [Pressure fix] repeated multi-producer tests show unrelated progress. [Races] exercises 256 seeded rounds. | Complete flooder/slow-consumer/native/sink/storage-pressure load with measured control latency, wakeups and no-starvation remains missing; full attached trial set failed overall. |
| G1-13 | partial | Typed quota boundaries, impossible configuration rejection, completed-session retention and abandoned observer/input/transfer lease tests pass [M2 log]/[L log]; [I/O fault evidence] adds exact transient permit release. | Concurrent at-limit allocation inventory is not exhaustive for every registry/metadata/helper/discovery/worker resource; failed/exhausted release population sweeps remain pending. |
| G1-14 | partial | `callback_panics_do_not_orphan_children_or_break_other_sessions`, failed-reader, helper-loss/isolation, process-future panic, and new read-panic tests pass [M2 log]/[L log], [Acceptance review], [I/O fault evidence]. | Each partial-spawn/acquisition/OS failure seam is not covered. Native process faults are not Rust panics and are not confined by this mechanism; unresolved seed 201 prevents any whole-runtime fault-isolation claim. |
| G1-15 | partial | Real child size acknowledgement, ordered model controls, explicit OS/model split outcomes, model-failure tests and new admission/completion failure generation retry pass [M2 log]/[L log], [I/O fault evidence]; [Races] includes resize/exit. | Complete parked/restoring/native-reference control races and five successful full-load latency repeats remain unqualified. The API must retain explicit partial outcomes rather than claiming OS/model atomicity. |

## G2: real terminal engine and FFI

| ID | Status | Executed evidence / current implementation | Remaining proof or scope limit |
| --- | --- | --- | --- |
| G2-01 | proved | Pinned real static Ghostty/Zig builds and exact verifier/library/patch identities are exercised in [M2 log]; [M2 metadata] includes current source inventory, [Native patch] and [Bitmap integration] bind the latest correction. Earlier [L log] exercises its explicitly older native identity. | This proves actual linkage/build and ownership operations for recorded native builds. Matching generated/bridge ABI evidence and cache verification do not establish semantic completeness or execution on other targets. |
| G2-02 | partial | Raw versus projected creation is explicit; real detached model/park/restore and single-authoritative query-reply tests pass [M2 log]/[L log]. [Ordered transfer review] checks exact byte/control boundaries. | Bounded scenarios establish exact-once behavior, but the full randomized uninterrupted-reference corpus is incomplete and has an unresolved native crash; do not generalize those scenarios to every sequence. |
| G2-03 | partial | Real native contracts cover split continuation, wide/combining/style/palette/alternate state and independent pending-wrap/row-wrap/cutoff regressions in [M2 log]/[L log]; [Native review]. | All-boundary and seeded randomized sequence qualification is unfinished. Seed 201 remains a recorded unresolved native process crash; neither selected regressions nor 100% owned-C counters prove arbitrary malformed/excessive input safety. |
| G2-04 | partial | Exclusive owner and synchronous callback/lifetime reviews, thread-moving/drop tests, allocator checks and current C boundary ASan/UBSan pass [M2 log], [C boundary review], [Native instrumentation]. | The linked Zig archive is not fully sanitizer-instrumented; Rust cancellation/lifetime tests do not isolate native faults. Exhaustive active-FFI cancellation and whole-engine instrumentation remain unproved. |
| G2-05 | partial | Real query child with multiple observers gets one response; queued reply admission, partial response no-resend, reply overflow, disabled image/options and callback cleanup are tested in [M2 log]/[L log], [C boundary review]. | Complete query-flood overload and clipboard/desktop/image effect-policy matrix at sustained release load remain missing; synthetic boundary faults do not prove all engine feature behavior. |
| G2-06 | proved | Domain views expose finite cells/styles/colors/modes/cursor and processed/control labels; cancelled/copied view budget tests and real native view/palette/wide-grapheme cases pass [M2 log], [L log], [READY review]. | Proved for exposed view contract and bounded extraction. Views remain rendering data, not restore payload; arbitrary full-screen clients and corpus completeness are separate requirements. |
| G2-07 | partial | Unfinished UTF-8/CSI/OSC/DCS checkpoint continuity, exact compatibility rejection, continuation caps and repeated roundtrips pass [M2 log]/[L log]; [Native patch] records fingerprinted semantic fixes. | Binary continuation tests do not establish all hidden engine state. Re-run full supported corpus and current patched native suites before a broad lossless-checkpoint claim. |
| G2-08 | proved | [READY review] and [M2 log]/[L log] prove READY/live FIFO/history alternation, skipped-history reporting, delayed FINISH/End, retained encrypted source and rebound limits/callbacks. `ready_applies_exact_64_byte_suffix_before_history_and_retains_encrypted_source` covers 8192+64 bytes through real engine/AEAD/disk/coordinator. | The 8192+64 case injects process events, not an actual PTY workload. Skipped history is explicit incompleteness, never complete-history success; capacity and long-load timing are separate G4 proofs. |
| G2-09 | partial | Real compression preserves full formatted history/wide/combining state; optional unsupported compression is typed; [M2 log]/[L log], [C boundary review] and [Packed experiment] contain executed scoped proof. | Integrated bounded compression scheduling latency, unprofitable-page policy and current replacement-engine contract matrix are not fully qualified. Experimental packing numbers do not select production policy. |
| G2-10 | partial | Native requested-byte, reply, view, continuation/checkpoint caps and exact alignment callbacks are tested [M2 log], [Native instrumentation]. [Packed experiment] separates logical/requested/mapped/physical memory in a native fixture. | Physical page/allocator overhead and production raw/empty/filled/projected/helper/reader resource accounting are not a complete integrated release measurement; candidate allocator changes need fresh source-bound measurement. |
| G2-11 | incomplete | A runnable seeded bounded corpus exists and earlier corrected failures have preserved regressions; [Loop4 summary] explicitly records unresolved seed 201. [Native instrumentation] states engine-wide sanitizer limits. | Complete the required duration/iterations/corpus with every failure resolved and reviewed. Investigation of the known crash is paused. No new reproduction or malformed-input work was performed for this audit, and no containment claim is made. |

## G3: parking, encryption, storage and ordered transfer

| ID | Status | Executed evidence / current implementation | Remaining proof or scope limit |
| --- | --- | --- | --- |
| G3-01 | proved | Default private disk composition, injected same-contract provider, finite options, raw skip, deterministic threshold and real 60-second parking evidence exist in [Default parking], [M2 log]/[L log], [Checkpoint contracts]. | Proved for recorded default/injected-store construction and eligibility behavior; permissions/filesystem assumptions and unsupported owner-restart recovery remain explicit. |
| G3-02 | partial | Real detached/attached parking, reader retention, immediate output-triggered restoration, parked input/exit/cancel and separate observer policy have bounded tests [M2 log]/[L log], [Default parking]. | Full idle/active/mixed population reader/resource and buffer-release measurements are missing; observer count is not a throughput/idle classifier. |
| G3-03 | proved | Domain attempt/lifetime/byte/control generations and encode/commit barriers, resize-only order, stale completion and close immutability pass [M2 log]/[L log]; [Ordered transfer review], [READY review]. | Proved for explicit atomic state transitions and deterministic interleavings; whole release traffic combinations remain in G4 rather than being inferred from these tests. |
| G3-04 | partial | Encode barrier stages output while retaining original owner; immutable I/O leases, rejected blocking admission, callback panic and transfer pin tests pass [M2 log]/[L log], [I/O fault evidence]; lock-scope reviews support storage outside native/global ownership. | Full slow encoder/consumer/storage population peaks and unrelated-session OS-control timing under load remain unmeasured; fixed caps alone do not prove acceptable responsiveness. |
| G3-05 | partial | Output during encode/commit retains live model, stale storage is deleted, uncertain commit remains charged, and failed publication does not release sole state; [M2 log]/[L log], [Shutdown evidence], [I/O fault evidence]. | Not every encoding/encryption/disk/cancellation/acquisition interleaving is independently injected. Selected red/green and exact source/reservation assertions must not be reported as exhaustive transaction proof. |
| G3-06 | partial | Separate parser/replay quotas, all-or-none admission, requeue-on-native-error, ordered restore and unrelated-session capacity notifications pass [M2 log]/[L log]; malformed read retains exact queued bytes [I/O fault evidence]. | Complete replay-eviction/restoration/backpressure stress with observed per-session stalls, global peaks and control/reaper latency remains absent from successful release load evidence. |
| G3-07 | partial | [READY review] proves exact 8192+64 applied suffix before old history and preserved source; application FIFO/history and coalescing contracts pass [M2 log]/[L log]. | The exact boundary test injects process events. Real PTY release-to-restore/history barriers plus separate first-byte/READY/full-history latency across five full repetitions remain unqualified. |
| G3-08 | partial | Real parked raw input, query-triggered output restore, FIFO resize and READY mutation contracts pass [M2 log]/[L log]; [I/O fault evidence] adds exact failed resize generation retry. | Model-dependent input and complete parked/restoring output/resize/history combinations against uninterrupted native reference have not been exhaustively qualified; release timing remains missing. |
| G3-09 | partial | Typed wrong compatibility/authentication/unavailable/truncated source errors, failed-view settlement, retained source/backlog and no empty reset are covered by [Checkpoint contracts], [READY review], [I/O fault evidence], [M2 log]. | Cannot claim all malformed native checkpoint inputs fail safely while the recorded native crash remains unresolved. Complete independent failure matrix, raw child/cancel survival and bounded pressure combinations remain required. |
| G3-10 | partial | Real AEAD/store contracts assert ciphertext-only payload, distinct nonces, wrong key/tampered tags/descriptors and opaque-envelope rejection; [M2 log]/[L log], [Checkpoint contracts], [protector source]. | These are strong scoped crypto-integration assertions, not an independent complete security/key-lifetime audit or every synthetic marker path. Runtime key cleanup and no restart recovery must remain documented. |
| G3-11 | partial | Real disk and injected provider contracts, partial/zero writes, concurrent quota, immutable identity/read bounds, lost/uncertain completion, delete failure and new read callback classifications pass [M2 log]/[L log], [Checkpoint contracts], [I/O fault evidence]. | Every cancellation/full-disk/delete/replacement fault interleaving is not proven. Native corruption behavior is separate from storage authentication and remains an explicit blocker to broad restore qualification. |
| G3-12 | partial | Bounded private arena cleanup now exists: live owner skipped, SIGKILL abandoned bytes reclaimed, partial scan blocks admission, symlink/hardlink/permissions rejected; [Crash cleanup], [Crash cleanup review], [M2 log]/[L log]. Transient/uncertain leases have tests. | The old claim of no crash cleanup is obsolete. Complete concurrent peak accounting and all filesystem/cancel failure combinations remain incomplete; unexamined abandoned bytes are not represented as a new owner's reservation. |
| G3-13 | partial | Real ordered transfer/reconnect and parked immutable attach plus domain gap/foreign cursor and slow pin tests pass [M2 log]/[L log]; [Ordered transfer review] and [Acceptance review] include cancelled in-flight ownership. | Broader real attach/detach/encode/commit/output/resize barriers, many slow consumer load and repeated explicit resync/reference reconstruction at release capacity remain unqualified. |
| G3-14 | incomplete | Shared bounded scheduling and real frees exist; [Packed experiment] records 120 native lifecycle cases and allocator sanitizer stress with physical measurements. It explicitly remains a single-threaded prototype and has production-adaptation blockers. | Production reclaimable pool/page-packing choice is not implemented/qualified. Resolve alignment/page-size/NDEBUG/fragmentation concerns before promotion, then measure per-owner integrated cycles, peaks/RSS/charged/PSS/disk and Linux behavior. |
| G3-15 | partial | Finite failed-park/delete retry and uncertain-reservation tests pass [M2 log]/[L log]. Native experiment records encode/READY/history timings; [Diagnostics review] distinguishes actual I/O/feed/control boundaries. | No complete integrated encode CPU/compression/cached-vs-uncached disk/READY/full-history/first-output matrix or long outage resource plateau. Do not equate native fixture timings or general histograms with every required stage. |

## G4: release workloads, platforms and deliverables

| ID | Status | Executed evidence / current implementation | Remaining proof or scope limit |
| --- | --- | --- | --- |
| G4-01 | partial | [Load summary] records the exact 64-resident/16-active, aggregate 10 MiB/s, five 60-second attached trials. Trials 2 and 5 passed; 1, 3 and 4 failed. [Load diagnosis] preserves harness red/green fixes now checked in [M2 log]. | The attached case is failed overall, not passed or unrun. Fresh full runs are required after harness fixes; detached/stalled observer/stalled sink/dominant producer cases are not complete. Do not replace original failures or reduce thresholds. |
| G4-02 | not-run | Standalone 128-producer experiments are listed in [Requirements]; integrated scripts implement population options, and bounded functional tests exist. | No stored full integrated 1/32/128 marginal sweep, 128 independently active/mixed matrix, per-producer fairness/rate/chunk/observer/grid sweeps found. 500 remains conditional, unexecuted and unclaimed. |
| G4-03 | partial | Fixed caller-owned stage histograms and actual write/read/feed/resize/cancel acknowledgement tests exist [Diagnostics review]. Two successful full attached trials meet their measured thresholds [Load summary]; failure records remain. | Need at least five successful post-warmup 60-second repetitions per required case, with complete distributions/counts/failures/timeouts. Harness fixes and two successes do not discharge the five-repeat criterion. |
| G4-04 | not-run | Quotas and aggregate snapshots are implemented and scoped repetition/resource samples exist [Repetition], [Diagnostics review]. | No full 64-idle/60-second ≤1% core and ≤4 KiB control-metadata proof found; no complete production stack/scratch/shared/physical resource decomposition or sustained-stall matrix. Requested allocation totals include fixture bookkeeping and are not this proof. |
| G4-05 | partial | Source/build/OS/compiler/native identities, run seeds/limits, RSS/FD samples and latency distributions are retained [Release build], [Load summary], [Loop4 summary]. Historical independent cache portability execution is retained [CI portability]. | No reviewed current same-host independent performance baseline with complete copy/allocation/syscall/wakeup/lock-contention accounting and repeatable >10% regression analysis. Self-comparison remains insufficient. |
| G4-06 | proved | [Repetition] records actual 10,000 real spawn/exit-or-cancel cycles, one seeded 4096-byte verified burst per cycle, explicit forget, final FD8=baseline8, no child/zombie at quiescent censuses and zero asserted reservations; exact frozen release image is bound by [Release build]. | Proved count and observed bounded cleanup on macOS for that frozen build. The 65.8-second run has zero post-120-second-warmup samples, so no long-term memory plateau is claimed; current candidate and other platforms need separate qualification. |
| G4-07 | proved | [Repetition] records actual 100,000 attach/drop operations with unchanged PID/lifetime/cursor and successful reacquisition of all observer slots; [M2 log]/[L log] separately cover cancelled-read cursor/permit behavior. | Proved requested attach count for the recorded quiet-child scenario; it is not 100,000 active-output/eviction or polled-abandoned-read iterations. Current-source/platform extension remains explicit. |
| G4-08 | partial | [Races] records all 256 seeded cancel/exit, resize/exit, attach/eviction and shutdown/spawn rounds; [Pressure fix], [M2 log]/[L log] add flood/helper/storage/waker/callback fault cases. | Full combined stopped-input/query/sink/snapshot/resource-exhaustion and repeated cold/warm pressure with bounded surviving-session latency is not yet release-qualified. A 3.75-second race run is not sustained stability. |
| G4-09 | not-run | [Stress runner] has an actual mixed-runtime twelve-hour mode and explicitly reduced parking/restore pilots. | No completed twelve-hour full run with post-warmup timeseries, per-session byte/gap ledger and plateau review exists in stored evidence. Configured runner, sleeping or short pilots do not count. |
| G4-10 | proved | [Owner death] records25/25 macOS and 25/25 Linux abrupt public Runtime-owner SIGKILL cases, verified session/foreground/background/helper-group roles, high non-CLOEXEC FD closure, no live original-session member, and surviving intentional escape/outside control. | Proved focused raw owner-death/session cleanup contract and documented escape limit; no snapshots restore a running process after owner restart. Projection death combinations and full fault/soak remain separate. |
| G4-11 | partial | Matching-source macOS arm64/Linux x86_64 gates passed [M1 metadata]/[L metadata]; interactive and owner-death smokes executed both. Latest candidate2 has only a macOS pass [M2 metadata]. [CI portability] records earlier actual GitHub Ubuntu/macOS/MSRV jobs. | Candidate2 Linux is not yet executed at audit cutoff. macOS x86_64/Linux arm64 remain unqualified; full release load/repetition/soak/native reclamation are not complete on all claimed targets. Narrow support only by explicit reviewed decision. |
| G4-12 | partial | Pinned real event-stream adapter, actual subscription/reconnect, sink-full/stale identity/cancelled retry, DTO decode and no-implicit-persistence tests pass [M2 log]/[L log]; [Event review], [Acceptance review]. | Full-load stalled sink bounds/isolation/latency and five-repeat workload matrix remain missing; adapter tests and configured optional feature do not prove aggregate release behavior. |
| G4-13 | partial | Same-source loop4 Mac/Linux mechanical gates pass; fresh candidate2 Mac gate passes95.58s unchanged source [M2 metadata]/[M2 log]. Includes formatting, strict lint, all-target/all-feature tests, raw/event-disabled paths, docs/examples, native boundary and load diagnostic regressions. | Fresh candidate2 Linux gate and final reviewed committed CI are pending. Four-target execution/cross-build scope must remain explicit; earlier CI at52ad04c is not current uncommitted candidate qualification. |
| G4-14 | partial | Typed facade/lockfile, native prerequisites/notices/Rustdoc, [Usage], interactive/event examples and [Interactive] seven-case real-host smokes per platform exist; [workspace integration] provides a service ownership sketch. | Historical missing-example/docs claims are obsolete. A concrete workspace-sdk integration deliverable and review of distributable guardian/native packaging/signing/support claims remain unproven; complete current-source docs/package acceptance after final reviews. |
| G4-15 | partial | Bounded fixed histograms, aggregate ownership/retention/quota counters, explicit unavailable/failure metrics and acknowledged cancel/escalation semantics exist; reviewed and tested [Diagnostics review], [M2 log], [I/O fault evidence]. [Usage] distinguishes caller-held memory and physical metrics. | Complete synthetic-secret audit across all Debug/error/log/metric paths, no unbounded-label proof for the whole public surface, and diagnostics overhead under full release workload remain incomplete. Host escalation requests and acknowledged successful syscalls are not signal receipt. |
| G4-16 | partial | Historical failures, scoped gate inventories, exact release image/run identity, trial summaries, source-bound reviews and raw red/green logs are retained under this verification tree; [Loop4 summary], [Load diagnosis], [CI portability]. | Final gate-specific release report under docs/experiments is still missing, including accepted baseline/threshold decisions, complete platform/workload/coverage results and resolved final reviews. Historical reports must remain historical, not overwritten as passes. |

## Architecture and user workflow

| ID | Status | Executed evidence / current implementation | Remaining proof or scope limit |
| --- | --- | --- | --- |
| A-01 | proved | Cargo dependency/metadata and std-only core gates pass [M2 log]/[L log]; [DDD review] records inward dependencies and [M2 metadata] binds manifests/source. Public facade composes infrastructure outside domain/application. | Proved current inspected crate direction and recorded core build contract, not a substitute for runtime/native qualification. |
| A-02 | partial | Domain lifetime/cursor/admission/generation/transfer/history-state tests and [DDD review] establish aggregate ownership. Current fault tests preserve domain error/lease policy instead of introducing native algorithms [M2 log]. | Final native semantic/corpus correctness and complete transaction/race matrix remain open; architecture review cannot imply every state transition combination is qualified. |
| A-03 | proved | [DDD review], [Organization review], [Event review], [Guardian review] and source inspection establish cohesive I-prefixed external ports, concrete internal policy, bounded atomic repository and direct SessionContext hot path. | Proved inspected boundary/ownership structure; re-review materially changed ports or lifetimes, and keep product behavior/resource qualification separate. |
| A-04 | partial | Engine-neutral domain types, opaque compatibility/native patch identity, required capability rejection and typed optional compression are implemented/tested [M2 log], [Native patch], [DDD review]. | Complete replacement-engine admission/no-process-allocation and existing-state incompatibility contract suite is not evidenced for every required capability. The unresolved engine failure is not cured by typed error mapping. |
| A-05 | partial | External cell/process/checkpoint/event conversions remain infrastructure-local; invalid range/enum/envelope and synthetic Debug redaction cases pass [M2 log]/[L log]; [Event review], [C boundary review]. | The full exported API/conversion/required-field/compatibility and secret-surface matrix remains incomplete. No unsafe/native DTO should be moved into domain to work around a failure. |
| A-06 | partial | Common application ports isolate platform reactor/watch and native details; actual matching-source process contracts passed on macOS/Linux [M1 metadata]/[L metadata], [DDD review]. | Latest Linux candidate, full adapter overhead/resource/physical-counter measurement and other target execution remain pending; RSS/PSS/charged/virtual values are not interchangeable. |
| A-07 | partial | Separate DDD, organization, correctness, guardian, READY, storage, diagnostics, load and bitmap reviews exist with explicit dispositions: [DDD review], [Organization review], [READY review], [Diagnostics review], [Load diagnosis], [Bitmap integration]. | Final candidate review consolidation still required; older reviews explicitly predate newer fixes. Experimental packing blockers and unresolved native crash cannot be silently closed by a structural review or gate pass. |
| A-08 | proved | [Coding standards] and mechanical gate enforce fmt/lint/tests/dependencies/350-nonblank Rust/C/header inventory; [M2 log]/[L log] record execution. Reviewer obligations that automation cannot establish are explicit. | Proved required always-run workflow mechanism and recorded use; each later completed loop still needs its own unchanged-source gate and independent review, not reuse of this verdict. |
| A-09 | incomplete | [CI portability] records private main push at52ad04c and authoritative successful GitHub runtime/experiment jobs. Current work remains a later dirty/uncommitted candidate bound by file manifests, not by HEAD alone. | Verify final private remote, commit/push reviewed source/docs/evidence to main, record clean tracked state and actual final CI. Historical52ad04c visibility/CI does not publish or qualify current work. |
| A-10 | incomplete | Independent acceptance/I/O/shutdown/native boundary/bitmap reviews and genuine defect red/green exist [Acceptance review], [I/O fault evidence], [C boundary review], [Bitmap integration]. [Coverage] is an executed failing strict target: workspace91.681%lines/84.606%functions/89.322%regions; helper11.594%/16.038%/13.608%. Owned-C combined boundary coverage is100% in its four-file scope. | 100% whole inventory target is unmet; Rust branches/MC/DC, upstream native, Python/build tooling and other platform configurations remain unmeasured. New tests need a fresh frozen report; synthetic own-C hits do not establish real-engine behavioral completeness or override any other row. |

## Required work before release acceptance

1. Resolve and independently qualify the known native correctness blocker within
   the authorized investigation scope; complete the required source-bound corpus
   and clearly bounded native instrumentation. Do not use branch coverage as a
   substitute for a correct roundtrip or process-failure contract.
2. Finish current Linux/platform source-bound mechanical/native qualification and
   final specialist review. Preserve old passing sources and failed attempts as
   separate evidence; do not claim unexecuted macOS x86_64/Linux arm64 support.
3. Execute corrected full attached load and every remaining required case with
   five 60-second post-warmup repeats, then the 128-active/mixed capacity and idle
   resource sweeps. Review baseline regressions and complete stage/physical-memory
   accounting. Keep the two passing old trials and three failures intact.
4. Complete production reclamation/pool decision and its per-owner/platform proof,
   remaining acquisition/failure/secret/adapter contract inventories, the strict
   coverage inventory/target, and missing package/workspace integration deliverables.
5. Execute and review the full twelve-hour mixed soak after deterministic checks
   pass. Publish gate-specific final evidence, commit/push reviewed main and inspect
   actual final CI. The already executed count milestones need not be described
   as absent, but any source changes affecting them require justified reruns.

Dynamic reader handoff remains explicitly deferred, neither passed nor an initial
release blocker. 500-session qualification remains conditional on available host
capacity; 128 independently active integrated sessions remains required.

[M1 metadata]: loop4/macos-gate-attempt3/metadata.json
[M1 log]: loop4/macos-gate-attempt3/command.log
[L metadata]: loop4/linux-gate-attempt2/metadata.json
[L log]: loop4/linux-gate-attempt2/command.log
[M2 metadata]: loop4/macos-gate-candidate2/metadata.json
[M2 log]: loop4/macos-gate-candidate2/command.log
[Loop4 summary]: loop4/README.md
[Release build]: release/full-build-1/metadata.json
[Repetition]: release/repetition-full-1.jsonl
[Races]: release/races-full-1.jsonl
[Load summary]: release/load-attached-full-1/summary.json
[Load diagnosis]: ../reviews/load-harness-readiness-diagnosis.md
[Stress runner]: ../../scripts/release/README.md
[Interactive]: interactive/README.md
[Owner death]: guardian/owner-death/README.md
[Pressure fix]: ../reviews/process-pressure-reset-race.md
[Diagnostics review]: ../reviews/runtime-diagnostics-process-review.md
[READY review]: ../reviews/ready-restoration-correctness.md
[Checkpoint contracts]: ../../crates/infrastructure/tests/checkpoint_contract.rs
[Crash cleanup]: checkpoint-crash-cleanup.md
[Crash cleanup review]: ../reviews/checkpoint-crash-cleanup-independent.md
[Default parking]: loop3/default-parking/metadata.json
[protector source]: ../../crates/infrastructure/src/checkpoint/protector.rs
[Guardian review]: ../reviews/guardian-correctness.md
[Guardian design]: ../reviews/guardian-production-design-review.md
[Ordered transfer review]: ../reviews/ordered-transfer-correctness.md
[Acceptance review]: ../reviews/independent-acceptance-tests.md
[I/O fault evidence]: projection-io-faults/application-tests.log
[Shutdown evidence]: coverage-shutdown-tests/metadata.json
[Native patch]: ../../scripts/native/patches/README.md
[Native review]: ../reviews/terminal-wide-cutoff.md
[C boundary review]: ../reviews/native-boundary-independent.md
[Native instrumentation]: native-instrumentation/README.md
[Packed experiment]: packed-pages/prototype-v1/README.md
[Bitmap integration]: ../reviews/bitmap-capacity-integration-review.md
[Coverage]: coverage-matrix-1/summary.json
[CI portability]: ci-portability/README.md
[DDD review]: ../reviews/loop4-ddd-consolidated.md
[Organization review]: ../reviews/loop4-organization-consolidated.md
[Event review]: ../reviews/event-stream-correctness.md
[Coding standards]: ../../coding_standards.md
[Usage]: ../usage.md
[workspace integration]: ../usage.md
[Requirements]: requirements.md
[command contracts]: ../../crates/infrastructure/tests/process_contract.rs
[raw contracts]: ../../tests/raw_runtime.rs
[replay source]: ../../crates/domain/src/replay.rs
[Input reservations]: ../../tests/input_reservation.rs
