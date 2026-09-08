# Remaining acceptance work at checkpoint 71c1d6f

Time-sensitive status refreshed as of 2026-09-08T21:31:52+00:00.

Release acceptance remains incomplete, but most non-native runtime behavior is implemented. The useful next steps are bounded missing-proof work, measurement attribution and remaining workload cases—not rebuilding the runtime or repeating completed milestones. This read-only refresh maps all 67 ADR ledger rows without rewriting the historical `requirements.md` or claiming a blanket pass. Native crash/corpus investigation remains user-deferred.

## New evidence that supersedes the earlier audit

- Candidate2 completed five attached trials on both macOS and Linux; macOS also completed five each detached, stalled-observer and stalled-sink trials. The earlier dominant fifth failure remains retained.
- Candidate3 retained45 attempts: five failed projected capacity, five passed raw capacity, and35 passed idle/resource trials (idle64 plus raw/projected1/32/128). It predates the control admission fix and reader gauges.
- Candidate4 (`487ef087`) completed five projected-capacity and five dominant correctness trials. Dominant latency targets pass all five; capacity projected-output p99 misses20 ms in all five and resize p99 misses100 ms in one. These were contention-affected observations. The independent parser/control quota defect has deterministic local/shared red/green proof and no admission exit in these repeats.
- The actual 10,000 lifecycle cycles, 100,000 attach/drop operations, 256 seeded race rounds and 25 owner-death cases per host are already executed. Preserve their source/platform limits rather than calling them unrun.
- Candidate5 Linux gate and release build now both pass at `71c1d6f`, with the unchanged 293-file inventory matching the macOS build/reviewed gate (`docs/verification/release/candidate5-linux-gate/`). The new full 12-hour soak started at 2026-09-08T21:15:08Z (remote session 259999) but failed after approximately 146 seconds, at 21:17:35Z, with `Projection(Worker)`; discovery occurred on the 21:29Z poll. Raw evidence is retained in `docs/verification/release/candidate5-soak-failed.jsonl`. No native abort or panic was reported. Last progress recorded 576 turns, 590875 verified bytes, 1768421 gap bytes and 560 parked observations. There is no current soak and no twelve-hour pass. Read-only Rust coordinator/fixture diagnosis is underway; native C investigation remains deferred. The earlier~40-minute attempt remains intentionally superseded. Root's frozen candidate5 depth sweep remains separately owned work.
- CI 71c1d6f attempt 1: experiments, macOS and MSRV pass; Ubuntu fails at the first survivor spawn in `process_failure_isolation::sentinel_loss_reclaims_known_descendants_and_preserves_another_session`, returning Io before intentional helper damage. The dedicated Linux gate passes separately. Failure evidence is retained in `docs/verification/ci-71c1d6f-attempt1/`; a failed-job rerun was requested after retention and has no verdict at this cutoff. Cause remains unestablished; native_instrumentation is performing read-only diagnosis.
- Fresh macOS strict Rust collection at `docs/verification/coverage-candidate5/` completed three feature test phases and helper units with unchanged 293-source inventory. Reports fail: workspace 7801/8401 lines (92.858%), 744/866 functions (85.912%), 11278/12460 regions (90.514%); helper **unit-only** 168/1449 lines (11.594%), 17/106 functions (16.038%), 301/2212 regions (13.608%). These exports include the tool's scoped test code and are not a filtered production-only denominator. Rust branch/MC/DC is unsupported/unmeasured here. Other-platform instrumentation is not established. Separate helper functional probes have behavioral execution, but their LLVM coverage exports are **rejected/untrustworthy** due saturated/inconsistent counts (`guardian/continuous-coverage.md`); no functional coverage percentage is accepted.

## Prioritized actions that can advance without native crash work

**R1 — Bounded non-native error/ownership proof, driven by the fresh report.** Start with uncovered application projection I/O/native-orchestration paths (despite the filename, `application/projection/native.rs` contains portable orchestration), runtime context and infrastructure process guardian/I/O/backend. Build a finite branch-to-contract list, then add only missing behavioral assertions: acquisition failure before child launch, registration/thread/FD rollback, short-write/EINTR/EAGAIN outcomes, cancellation/completion ownership and exact permit release. Existing admission rollback, contradictory completion, replay reclamation, parser/control capacity, reader unwind and lost-completion tests are completed additions, not new tasks. Keep real OS probe assertions separate from unit mocks. Reliable helper functional coverage collection is a tooling gap; do not claim current rejected exports satisfy100%. This is primarily proof debt, not evidence that each uncovered line is defective.

**R2 — Finite public error/conversion/secret matrix.** Finish a named export/DTO/capability table with invalid/absent/foreign values and synthetic marker expectations, using existing tests wherever present. Verify only missing boundaries: stable portable error category, rejected replacement engine without process/reservation side effects, no command/env/output marker in Debug/error/metric records, and key cleanup assumptions. No new security flaw is asserted by this inventory. G4-14 already has docs/examples; this is bounded completeness proof, not an instruction to broadly re-review unchanged source.

**R3 — Storage/parking orchestration edges using deterministic providers.** Enumerate remaining encode/protect/commit/read/delete/cancel interleavings around existing manual schedulers and provider fault seams. Require exact byte/control cursor, retained sole source, no duplicate commit/reply, released transient leases, and unrelated control progress. Include replay eviction during delayed restore, retained transfer pins and bounded outage retries. Authentication/unavailable/short-read paths can be proved before invoking native decoding; do not fuzz malformed native checkpoints. Existing default60-second parking, crash namespace cleanup, READY8192 + 64 and ordered transfer implementations are present. Real-PTY boundary timing and integrated stage metrics remain separate measurements.

**R4 — Finish the remaining load evidence after the active sweep.** Twelve default matrix cases have no retained full five-repeat result here: `128-active`, `128-mixed`, `rate-1MiB`, `rate-20MiB`, `rate-40MiB`, `chunk-1`, `chunk-64`, `chunk-65536`, `observers-4`, `observers-16`, `grid-160x48`, `grid-240x80`. Existing14 case histories across earlier candidates must remain visible; final-source reruns need a source-change rationale, not blanket repetition. Do not compete with or duplicate root's active depth sweep. Use its throughput/upstream blocking/RTT alongside parser p99, preserve default 256, and resolve the observed saturated-latency miss through reviewed evidence rather than quietly changing targets or defaults. Quiet same-host regression baseline remains unaccepted.

**R5 — Complete physical memory and stage attribution.** Actual reader scratch gauges are implemented; missing work is measured final-source attribution of per-reader virtual/resident stacks, shared runtime/helper/fixture/native costs, and the4 KiB control-state conclusion. Candidate3resource sweeps are valid earlier evidence, not final measured-reader proof. Add only necessary final-source measurements and derive deltas from existing records. Separate encoder/protector/store/READY/full-history/first-output timing where absent; do not label generic histograms as those stages. A production reclaimable pool policy remains unselected/unqualified; the native packing prototype is not production. Resolve evidence/design first and keep native allocator promotion outside the deferred investigation scope.

**R6 — Close documentation/contract deliverables with exact scope.** Reconcile the67 rows to source-bound evidence, produce the final gate-specific release report, and finish an executable workspace-sdk integration sketch/guardian-native packaging/signing/support review if required by the delivery scope. Existing service integration prose, interactive example, event example and notices must not be reported missing. For G3-08, define whether model-dependent encoding is an exposed operation; the current public byte-write path already handles encoded input. Do not invent a new input API merely because a broad historical row lacks an explicit applicability statement.

**R7 — Coordinator-owned final qualification.** Record the completed matching-source candidate5 Linux gate/build separately from the failed Ubuntu CI attempt and its pending rerun/diagnosis. Preserve already passed Mac/Linux scope; macOS x86_64 and Linux arm64 remain separately unqualified. The new soak failed at approximately 146 seconds with `Projection(Worker)`; preserve `candidate5-soak-failed.jsonl`, complete the bounded Rust coordinator/fixture diagnosis, and require a fresh full twelve-hour run after any reviewed fix. No soak is currently running. Current depth sweep and CI/soak diagnosis are owned work. Strict coverage is completed and failed, retained canonically. Final native proof remains blocked by the user's deferral and cannot be discharged by these non-native advances.

## All 67 rows: current disposition and exact next work

“Scoped proof retained” carries the historical executed scope; it is not a fresh whole-release verdict. “Implemented, proof partial” does not mean an unimplemented feature. Native-held rows are included for completeness but are not an instruction to investigate them.

| ID | Disposition | Remaining scope/action |
|---|---|---|
| G1-01 | Scoped proof retained | Command/environment/root/dimension contracts; extend only for changed acquisition/secret edges (R1/R2). |
| G1-02 | Scoped proof retained | Real PTY/echo/binary/interactive tests; no repeat requested merely for inventory. |
| G1-03 | Scoped proof retained | Atomic identity/lifetime tests; new admission rollback/waiter test strengthens failed-create reuse (R1). |
| G1-04 | Scoped proof retained | Exit/drain separation plus new contradictory-completion facts tests; remaining failure seams R1. |
| G1-05 | Scoped proof retained | Detach/cancelled-read identity and executed 100,000 attach count; do not rerun count by default. |
| G1-06 | Implemented, proof partial | Five detached/stalled-observer repeats now exist; allocation/fairness at remaining matrix extremes R4/R5. |
| G1-07 | Implemented, proof partial | Input reservation/partial outcomes exist; exact untested EINTR/EAGAIN/short-write/pre-copy boundaries R1. |
| G1-08 | Implemented, proof partial | Full controlled cases and cancellation probes now execute; abandoned caller/control-pressure edge inventory R1, remaining load R4. |
| G1-09 | Implemented, proof partial | Guardian authority and 25 owner-death repeats per host retained; untested acquisition/discovery/host-policy seams R1. |
| G1-10 | Implemented, proof partial | 10,000 cycles and shutdown tests retained; not every thread/FD/registration failure seam injected R1; soak R7. |
| G1-11 | Implemented, measurement partial | Actual reader capacity gauges now implemented/tested; virtual/resident stack and full physical attribution still R5. |
| G1-12 | Implemented, proof partial | Independent controls fixed with local/shared parser-capacity red/green; combined storage/sink pressure R3/R4. |
| G1-13 | Implemented, proof partial | New failed-projection creation reservation/waiter rollback; remaining at-limit acquisition matrix R1. |
| G1-14 | Implemented, proof partial | Rust callback/worker failures tested; missing non-native partial-acquisition seams R1. Native process fault remains held. |
| G1-15 | Implemented, proof partial | Resize independent admission and exact FIFO now proved; parked/restore orchestration R3, measured saturation miss R4. |
| G2-01 | Scoped native proof retained | Pinned build/source identities exist; no deferred native investigation reopened. |
| G2-02 | Native proof held | Exact-once bounded cases exist; full reference corpus is deferred. |
| G2-03 | Native proof held | Full split/malformed/corpus qualification remains deferred. |
| G2-04 | Native proof held | FFI ownership/sanitizer scope remains partial; no whole-engine isolation claim. |
| G2-05 | Native proof held; orchestration partial | Selected reply/effect cases exist; non-native reply admission bounds can advance under R1; full engine effect matrix held. |
| G2-06 | Scoped proof retained | Bounded copied view contract; broad native semantics remain separate. |
| G2-07 | Native proof held | Continuation/checkpoint semantic completeness needs deferred corpus. |
| G2-08 | Scoped proof retained | READY/live FIFO/skipped-history contract already tested; additional real PTY wake timing remains R3/R5, not absent implementation. |
| G2-09 | Native proof held | Compression/packing policy qualification is not selected by an experiment. |
| G2-10 | Implemented bounds, measurement partial | New reader gauges improve accounting only; physical resident/history/helper attribution R5, native allocator promotion held. |
| G2-11 | Deferred native blocker | Known corpus crash unresolved; do not run or repair it in this workstream. |
| G3-01 | Scoped proof retained | Default60-second parking/private disk/provider contracts exist; no parking implementation task needed. |
| G3-02 | Implemented, measurement partial | Resident1/32/128 sweeps exist; parked/mixed reader lifetime and resource deltas R3/R5. |
| G3-03 | Scoped proof retained | Generation/commit/byte/control barriers exist; do not duplicate them without a specific missing edge. |
| G3-04 | Implemented, proof partial | Immutable I/O reservations and lock-scope proof exist; slow provider/control-progress and peak cases R3. |
| G3-05 | Implemented, proof partial | Stale commit/model retention/lost completion tests exist; enumerate remaining storage/protection/cancel edges R3. |
| G3-06 | Implemented, proof partial | Parser-capacity control defect fixed; replay eviction during stalled restore/encoding and peak/control evidence R3. |
| G3-07 | Implemented, proof partial | Exact8192 + 64 READY test exists with injected events; real PTY boundary/timing proof still R3/R5 without malformed engine work. |
| G3-08 | API contract/proof partial | Already-encoded parked input exists; clarify whether model-dependent encoding is an exposed operation before calling it missing implementation (R6); ordered restore tests R3. |
| G3-09 | Implemented, proof partial | Non-native unavailable/authentication/short-read/backlog preservation can advance R3; malformed native checkpoint safety held. |
| G3-10 | Implemented, proof partial | AEAD/ciphertext/descriptor tests exist; bounded key-lifetime and marker-surface inventory R2/R3. |
| G3-11 | Implemented, proof partial | Disk/injected providers exist; untested full-disk/cancel/delete/replacement edges R3. |
| G3-12 | Implemented, proof partial | Crash cleanup/private namespace exists; remaining concurrent peak/FS-error rollback cases R3. |
| G3-13 | Implemented, proof partial | Real event transfer and parked immutable attach exist; slow retained pins/resync/cancellation peak cases R3/R4. |
| G3-14 | Production policy incomplete | Packing prototype exists but no accepted production reclaimable-pool choice/physical proof. R5/R6 first; do not promote native prototype in deferred scope. |
| G3-15 | Instrumentation/measurement partial | General diagnostics and bounded retries exist; integrated encode/protect/store/READY/history timing and long-outage plateau R3/R5. |
| G4-01 | Executed across earlier sources | All5 controlled modes now have successful five-repeat histories; candidate4 dominant fixes earlier failure. Final-source binding/remaining matrix R4, not blanket not-run. |
| G4-02 | Executed partly | Raw/projected1/32/128 resource sweeps and raw capacity complete; exactly12 default matrix cases lack retained full results R4. |
| G4-03 | Executed, thresholds partly fail | Candidate4capacity output p99 all 5 miss; resize 1 miss; dominant all 5 pass. Active depth sweep owned by root; do not duplicate (R4). |
| G4-04 | Executed partly, memory target unproved | Fiveidle64 CPU/resource runs pass oncandidate3; final measured-reader/stack/control-memory attribution R5. |
| G4-05 | Method implemented, baseline unaccepted | Contended host evidence is not quiet reviewed baseline; compare distributions and full overhead metrics R4/R5. |
| G4-06 | Scoped proof retained | 10,000 actual lifecycle cycles complete; only rerun if changed source materially affects this scope. |
| G4-07 | Scoped proof retained | 100,000 actual attach/drop complete; active-gap obligations separate. |
| G4-08 | Executed partly | 256 seeded races complete; remaining combined stopped-input/storage/query/sink pressure R3/R4, no duplicate generic race run. |
| G4-09 | Required workload failed | Candidate5 soak failed after approximately 146 seconds at 21:17:35Z with Projection(Worker); raw candidate5-soak-failed.jsonl retained. No current soak/no twelve-hour pass; bounded Rust diagnosis underway (R7). |
| G4-10 | Scoped proof retained | 25 abrupt-owner trials per Mac/Linux complete; escaped-descendant limit preserved. |
| G4-11 | Executed partly | Candidate5 Mac/Linux gate/build 293-file inventories match and pass. Candidate5 soak failed; other architectures/full workload scope remains R7. |
| G4-12 | Implemented, proof partial | Pinned adapter/reconnect and5 stalled-sink full repeats exist; missing extreme/combined pressure R3/R4. |
| G4-13 | Dedicated gates pass; CI partly failed | Candidate5 Mac/Linux gate/build pass. CI attempt 1 Mac/MSRV/experiments pass,Ubuntu survivor-spawn Io failure retained; requested rerun pending, cause unestablished (R7). |
| G4-14 | Deliverables mostly exist | Facade/lock/notices/docs/interactive/event examples and workspace service sketch exist; executable workspace-sdk sketch/package helper signing review R6. |
| G4-15 | Implemented, proof partial | Bounded gauges/histograms and real reader measurements exist; complete redacted public-surface inventory and overhead R2/R5. |
| G4-16 | Evidence mechanism exists | Final accepted per-row/platform/threshold/coverage report remains R6/R7; historical failures retained. |
| A-01 | Scoped proof retained | Inward crate dependency gate exists/passes; no architecture rebuild requested. |
| A-02 | Implemented, proof partial | Domain state transitions and new completion facts tests exist; missing exact non-native transaction edges R1/R3. |
| A-03 | Scoped proof retained | External ports/atomic repository/hot path reviewed; re-review only material changes. |
| A-04 | Implemented, proof partial | New replacement-factory failure now proves no process launch/permit leak; remaining capability/compatibility table R2. |
| A-05 | Implemented, proof partial | Infra conversions exist; required-field/portable-error/secret table R2. |
| A-06 | Implemented, platform partial | Common ports and two OS gates exist; physical-overhead and other architectures R5/R7. |
| A-07 | Review mechanism exists | Reader/control/flag independent reviews current; consolidate final source verdicts R6, keep native deferred. |
| A-08 | Scoped proof retained | Always-run gate mechanism and dedicated Mac/Linux gate passes retained; CI Ubuntu failure is separate evidence, not overwritten. |
| A-09 | Published checkpoint, final CI pending | 71c1d6f checkpoint exists; CI attempt 1 Ubuntu failed, other jobs passed; failed-job rerun requested with no verdict. Final release remains pending (R7). |
| A-10 | Strict target failed | Freshworkspace92.858/85.912/90.514 and helper unit 11.594/16.038/13.608. R1; helperfunctional coverage rejected, otherOSinstrumentation unmeasured. |

## Provenance and limits

Read `docs/verification/requirements.md`, the67-row historical follow-up and rows.json, current release README and candidate3/4 reports/summaries, current reader/control/coverage-contract proof, the reviewedMacgate/build metadata and fresh coverage exports. The source checkpoint is `71c1d6f`; candidate-specific results retain their own earlier identities. Completed candidate5 Linux gate/build and the failed candidate5 soak are retained separately; depth sweep and pending CI rerun have no new verdict here. No tests, builds, workload executions, native investigation or source edits were performed. Evidence hashes and the67-row mapping are retained in `docs/verification/remaining-acceptance-71c1d6f/`.
