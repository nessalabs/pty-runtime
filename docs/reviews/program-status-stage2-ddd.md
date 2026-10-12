# Program status stage two: independent DDD review

Reviewed 2026-10-11 against stage-one base `3490f98` and the changed production
sources listed below. Result: **PASS for domain ownership, dependency direction
and application admission contracts; no scoped P1/P2 finding remains.** This
does not replace the mechanical, adversarial, code-quality, performance or
coverage gates, and makes no release qualification claim.

## Ownership and boundaries inspected

- The domain retains grammar, bounded continuation, record replacement,
  eviction, inheritance, revision and lifetime rules. The added plain-input
  and snapshot reservation methods answer allocation questions without
  introducing application quotas, native types, clocks or operating-system
  dependencies into the domain.
- The coordinator owns `ProgramStatusState` separately from `NativeWorkspace`.
  Parking and restore cannot replace the store. The short protocol lock is
  instrumented as a leaf and is released before terminal calls; snapshot
  readers need no native lock. Application code owns admission and sequencing,
  while domain code performs the actual status transitions.
- `ProjectedProgramStatus` owns its quota lease and exposes borrowed records.
  It cannot detach the allocated snapshot from its charge through an owned
  extraction or automatic clone. Caller-requested copies have documented
  caller ownership. Session close releases store memory while retained
  snapshots retain their own charges.
- Scan scratch and potential retained records are admitted before native
  mutation. Query splitting reserves both aggregate and native reply capacity
  while preserving a single logical reply limit. Admission failure before
  feed may retry; failure after any native slice permanently fails projection
  and retains original diagnostic bytes without replaying the accepted prefix.
- The scan plan commits only after all native slices succeed. Queries generate
  the fixed reply after their terminating slice and before later native replies.
  Original bytes remain the journal payload and advance the processed cursor
  once per accepted original chunk. Raw sessions have no coordinator and keep
  the passive public API behavior.
- One preallocated queue slot owns the coalesced exit marker independently of
  reader and request admission. Worker dispatch handles that ordered marker
  before deciding whether a parked terminal needs restoration. Settled drain
  includes the marker in its readiness condition; a separate once-only drain
  transition handles reports admitted after exit, including drain-before-exit.

## Earlier design concerns and re-review

The earlier review identified integration traps before stage-two implementation:
an exit marker could require native restore, ordinary request pressure could
drop the marker, per-slice replies could exceed the aggregate budget or omit
temporary buffer charges, and the terminal port incorrectly promised a safe
`HistoryIncomplete` retry. The reviewed implementation addresses each one.
The terminal port now explicitly treats every feed error as potentially
mutating. No checkpoint compatibility or native implementation change was
introduced for protocol state.

The stage-one accounting findings remain fixed: boxed records are counted in
plans, persistent scanner allowance is retained, and malformed IDs cannot
silently target root. The new snapshot bound includes each record's owned
strings, record storage and the resolved-app copy allowance. The zero-scratch
path is limited to a committed ground-state scanner and input with no ESC;
it does not bypass a partial sequence's continuation.

## Independent evidence and limits

Ran `cargo test -p pty-runtime-application projection::tests::program_status
-- --nocapture`: **14 tests passed** after leaf-lock instrumentation was
present. The tests cover ordered query/native replies, split queries, partial
native failure without replay, aggregate reply overflow, exit under saturated
admission, both exit/drain orders, parked reads and exit under resident pressure,
snapshot lease lifetime, admission retry, impossible-budget failure, lazy
ordinary output, record-cap validation, restoration invariance and snapshot
reads while a native checkpoint is blocked.

These tests use the replaceable terminal/process boundaries to establish
application orchestration. Real PTY/native tests and final gate evidence are
recorded separately by the integrating reviewer. The full 100% coverage target
remains independently tracked in `program-status-coverage.md`; this review
does not waive unmeasured or uncovered paths.

## Reviewed source identity

SHA-256 `e82b0d4ca5448e4ee0e32aa79287ee23ffb41e48c08f712e790abd8861f26d35`,
computed by sorting the following paths and hashing each UTF-8 path, a NUL,
then its complete file bytes in order. This identifies reviewed production
content independently of concurrently edited tests and documentation:

```text
crates/application/src/projection/admission.rs
crates/application/src/projection/coordinator.rs
crates/application/src/projection/mod.rs
crates/application/src/projection/native.rs
crates/application/src/projection/output.rs
crates/application/src/projection/program_status.rs
crates/application/src/projection/queue.rs
crates/application/src/projection/state.rs
crates/application/src/projection/teardown.rs
crates/application/src/projection/worker.rs
crates/application/src/runtime/context.rs
crates/application/src/runtime/session_projection.rs
crates/application/src/terminal/mod.rs
crates/domain/src/projection/options.rs
crates/domain/src/terminal/program_status/scanner.rs
crates/domain/src/terminal/program_status/store.rs
src/lib.rs
```

Any subsequent production correction requires review of its affected contract;
this identity is not a blanket approval of future changes.

Final refresh: inspected the worker comment correction removing the obsolete
oversized-reply example; it changes no behavior and the PASS verdict remains.
Also inspected `/tmp/pty-program-status-runtime-final.log`, which records three
passing real PTY/native tests after the integrating reviewer added paired-view
and exact transfer-tail assertions. The final mechanical gate remains separately
reported; this documentation refresh changes no production source.

Coverage-test refresh: the manifest now includes the file bytes of the appended
`#[cfg(test)] counter_boundaries` module in `store.rs`; those tests and the
additional public coverage test were authored by this reviewer. They change no
production behavior or API, so the preceding production DDD verdict is retained.
This identity refresh is not an independent review of this reviewer's own tests.
The separate code-quality reviewer reviewed the additions and reported PASS;
the integrating task records that independent evidence. The public test file
is outside this 17-file manifest. Final coverage results remain in the separate
coverage audit, and a passing mechanical run does not imply 100% coverage.
