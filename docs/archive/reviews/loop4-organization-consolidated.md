# Loop 4 independent Clean Code / SOLID consolidation

Reviewed 2026-09-08 by the independent organization reviewer. No production edits
were made in this review. This consolidates the current loop-4 delta; it is not a
claim to have independently repeated every behavioral or release test.

**Result: no new concrete P1/P2 organization or dependency-direction defect was
found in the reviewed production structure. Acceptance remains open for the
changing native snapshot row-wrap correction and the explicit qualifications
below.** Existing experimental P2 findings are preserved, not silently resolved.

## Source and validation boundary

`docs/verification/loop4/organization-source.json` records 166 changed or new
implementation/build/test inputs against the dirty working tree and its base HEAD.
Its aggregate SHA-256 is
`f15841691f7abf84530157a7b6ea682f5d96ec5c6829ddd60b7a2fced5667008`.
The manifest specifies the digest construction and each file hash. It fingerprints
the delta, not the entire native dependency archive or every unchanged source.
Later edits require a delta comparison and focused re-review.

Independently executed `scripts/gate.py`'s `architecture()` check and the adversarial
size-gate unit test; both pass. Evidence: `docs/verification/loop4/organization-checks.log`.
Inspected the root-coordinated complete gate's retained
`docs/verification/loop4/macos-gate-attempt2/command.log`, which ends in
“Mechanical gate passed.” That is the root's run, not an independent rerun.
The subsequent native row-wrap work needs a new gate on its final source.

Source inspection covered the actual transfer/order/journal/snapshot and READY
worker paths; runtime context/observer/cancellation ownership; diagnostics ports
and counters; event-stream pending-state/receipt/codec boundaries; checkpoint
arena/directory/reclamation; helper build/image/launch/registration/lifecycle,
protocol, discovery, anchors, cancellation and cleanup; public composition;
native allocator/source-patch integration and the experimental packed-page pool.
Prior deeper reports were checked against those current boundaries:
`ordered-transfer-organization.md`, `event-stream-architecture.md`,
`guardian-architecture.md`, `guardian-correctness.md`,
`runtime-diagnostics-architecture.md`, `checkpoint-crash-cleanup-independent.md`,
`interactive-correctness.md`, `packed-pages-prototype.md`, and
`terminal-resize-roundtrip.md`.

## Structural assessment with concrete evidence

- **Transfer policy and ownership are separated without a generic repository.**
  `crates/domain/src/projection/transfer.rs` owns lifetime, sequence, retention and
  terminal-boundary validation. Application `projection/journal.rs:92,164` owns
  payload retention and bounded observer registration. Arc-backed records carry
  byte/slot leases, so eviction does not erase the charge for consumer-held data.
  `snapshot.rs:54,92` uses the existing serialized model owner and reserves the
  continuation before publication. `transfer.rs` returns borrowed payloads and
  separately owned checkpoint/observer parts. These types express real lifetime
  differences; they are not mirrored transport DTOs or an interface hierarchy.
  Journal waker clone/replacement/destruction occurs outside its mutex. The
  same correction is visible in runtime attachment/completion and projection
  ticket waits, rather than left as a consumer workaround.
- **The coordinator split follows work and lock ownership.** `worker.rs:8`
  serializes native work through the engine while admission uses the shorter core
  lock. `snapshot.rs` captures immutable state, `stream_end.rs` seals the applied
  prefix, `completion.rs` interprets external completions, and `io.rs` owns
  submitted storage operations. READY's alternating live operation/history unit
  belongs in that shared scheduler. It does not add an OS branch or require a
  frontend to reconstruct missing history. These modules share one coordinator
  because they operate on the same model lifetime; forcing a service per file
  would obscure serialization and does not solve a demonstrated defect.
- **Event-store dependency inversion is correctly located.**
  `crates/infrastructure/src/event_stream/publisher.rs:94,134` transitions from
  retained source to immutable prepared event and changes acknowledgements only
  after receipt validation. The explicit pending enum prevents advancing the
  source past failed conversion. The optional external sink API stays in
  infrastructure, including schema/status conversion and distinct store versus
  PTY cursors. Application remains std/domain-only. A new application port merely
  duplicating EventSink would add abstraction without moving a policy boundary.
  The existing receipt-regression P2 is visibly corrected by the monotonic offset
  check, not hidden by retries in the example.
- **Guardian complexity is partitioned by authority rather than file size.**
  `helpers/guardian/src/discovery.rs:23` obtains candidates; `anchor.rs:30` owns a
  verified member and child ledger; cancellation manages pinned foreground/root
  controls; cleanup owns resumable sweeps; role modules own the S/G/successor
  topology; `scripts/guardian/protocol.rs` is the single shared fixed codec.
  `crates/infrastructure/src/process/guardian.rs` translates validated helper
  messages and retains the direct sentinel/channel ownership. Launch and
  registration establish RAII before failures can escape; image materialization
  and target compilation are separate adapters. There is no application-level
  PID cache, discovery service locator or fallback signal policy. The previous
  anchor-setup and successor-endpoint ownership findings remain fixed in source.
  Multiple retained state flags reflect independently observed facts; replacing
  them with a single lifecycle enum would discard legitimate partial states.
- **Checkpoint reclamation has a coherent physical boundary.**
  `crates/infrastructure/src/checkpoint/file.rs:49` rejects incomplete startup
  inventory; `arena.rs` owns shared initialization locking; `inventory.rs` owns
  one directory stream; `cleanup.rs` owns bounded reclamation; `filesystem.rs`
  owns descriptor-anchored validation and operations. Domain owns limits and
  portable reports. No filesystem record or errno enters application policy.
  The earlier live-prefix admission P2 remains resolved by fail-closed admission.
- **Diagnostics observe existing ownership rather than becoming a second owner.**
  `crates/application/src/diagnostics/mod.rs:60,114` contains fixed aggregate
  storage and a single-outcome timing owner. Runtime/process call sites record
  admission versus actual adapter completion separately. Context destruction
  releases existing reservations even on poison; activity and retained replay
  are distinct gauges. Process sessions retain the diagnostics Arc without
  retaining the context/event owner. Neither metrics nor their reset control
  scheduling or quota policy. This concrete bounded value needs no plugin
  interface or generic metrics repository.
- **Native semantics stay in the shared engine boundary.** The allocator bridge
  decodes the pinned alignment ABI and honors alignment in native ownership code.
  Snapshot correction is a fingerprinted dependency patch, with source/build and
  compatibility identity in the existing native build path. It does not inject VT
  commands, rewrite opaque payloads in consumers, or branch terminal semantics by
  OS. That is the appropriate correction location. Correctness acceptance of the
  still-changing row-wrap patch is separately required; architecture cannot prove
  that a checkpoint preserves every hidden terminal state field.

## File organization and size enforcement

The independent inventory found no Rust/C/header file above 350 nonblank lines,
including examples, fixtures, native build helpers and the standalone helper.
The largest are `process/guardian.rs` at 345 and `runtime/context.rs` at 342.
Neither uses generated inclusion or compacted line formatting to hide a larger
implementation. Shared `scripts/guardian/protocol.rs` is counted and included by
both actual owners; it prevents duplicated wire definitions rather than evading
an inventory. Native/test helper additions are counted by the same gate, with a
unit test proving representative oversized files are rejected.

Those two near-limit modules are maintenance pressure points, not a current
350-line exception. Future additions should split by host protocol/admission
versus service polling, or context ownership versus process-event adaptation,
when the actual change establishes that need. This is an observation, not a P2
request for speculative restructuring now.

## Unresolved findings and acceptance requirements

1. **P1, native behavioral scope still changing:** root reports an additional
   snapshot row-wrap state-loss reproduction and a specialist is preparing the
   dependency/tests correction. `scripts/native/patches/snapshot-pending-wrap.patch`
   and `scripts/native/verify_source.py` in this manifest predate that final
   acceptance. Re-review the final patch, compatibility/cache identity, public
   continuation regressions and portable execution. Do not transfer an engine
   invariant into a consumer workaround or promote this report to closure.
2. **Existing P2s, experimental only, remain open:**
   `experiments/native/packed-pages.h:56` performs `munmap` inside `assert`, and
   its bitmap/page-size assumptions at lines 72–77 remain unsuitable for broader
   page sizes. Its accounting, alignment and per-owner lifetime adaptation
   requirements remain in `packed-pages-prototype.md`. The actual production
   allocator does not include this header. Keeping the prototype is acceptable
   as explicitly limited experiment evidence; production promotion is blocked.
3. **Qualification is not structural correctness:** guardian signed packaging,
   exhaustive resource/discovery failure injection, full capacity measurements,
   platform matrix/repetition and the 12-hour soak remain their proof-ledger
   obligations. The prior guardian design report's conditional status is not
   overwritten by this organization review. The macOS/Linux interactive smoke
   validates its bounded raw-client cases and does not qualify Ghostty or general
   output/paste saturation.

No additional confirmed structural P1/P2 was found. This report supports the
reviewed production organization while preserving these unresolved items and
the requirement for a final source-bound gate and independent native re-review.
