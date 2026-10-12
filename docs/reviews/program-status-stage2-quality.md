# Program status stage 2: independent code-quality review

Result: PASS for Clean Code, SOLID, organization, ownership and dependency direction.
No unresolved scoped P1/P2 findings. This review does not by itself pass the
mechanical, adversarial, performance, coverage or release gates.

Reviewed the stage 2 changes against `3490f98`, including new untracked production
and test files, after the reply-suppression mutation was restored. Source identity
from `scripts/performance.py::identity`:
`d2f25a26befc42f19780694ff55bdda411092c6cbafa3bcdd898dc12d5fa9199`.

## Findings and judgment

- No blocking architecture or design finding. Domain continues to own protocol
  parsing and record semantics. Application owns resource admission, serialized
  execution and lifecycle coordination through existing ports. No infrastructure
  dependency or second parser was introduced into application.
- `program_status.rs` coherently owns snapshot admission and session status state;
  `output.rs` coherently owns the feed transaction and ordered reply assembly.
  Their extraction keeps native command dispatch narrow without introducing a
  forwarding framework or speculative abstraction.
- Snapshot ownership is structural: `ProjectedProgramStatus` holds its lease,
  exposes borrowed records and cannot accidentally clone the lease through a
  derived Clone implementation. Explicit caller copies are documented as caller
  allocations. Cleanup releases store state while outstanding snapshots remain
  charged. Scan admission precedes feed, and rejected admission drops temporary
  leases without advancing the store.
- The dedicated preallocated exit marker slot and coalescing flag fit the existing
  ordered queue. Processing exit outside native restoration preserves the session
  lifetime boundary; final drain performs the second required cleanup. Failed or
  closed projection paths prevent the read API from advertising accepted status.
- Feed errors retain the original diagnostic chunk and stop projection. Partial
  feeds are not retried. Aggregate and per-feed reply buffers are admitted before
  native calls; aggregate failure clears transient reply bytes rather than sending
  a partial protocol response.

Nit: the output batching comment in `worker.rs` still uses oversized replies as
an example of failure falling through to an immediate schedule. The new
`apply_output` returns Dormant on that failure. The failure guard itself remains
useful and correct; this stale example is nonblocking prose cleanup.

## Evidence and re-review

Independently ran:
`cargo test --locked -p pty-runtime-application program_status -- --nocapture`.
All 14 selected tests passed; 114 unrelated application tests were filtered out.
The tests cover quota lifetime after close, native failure after a prefix,
reply aggregation/order, admission retry, full-queue exit, drain ordering,
park/restore and status reads during a blocked native checkpoint.

Inspected `/tmp/pty-program-status-runtime-after.log`: all three real-child
Ghostty tests passed. The orchestrator reported the reply-suppression mutation
failed the real-child reply test and was restored before this review. No new
P1/P2 fix was requested by this review, so a fix-specific re-review is not
applicable. New production changes require renewed review.

## Remaining limitations

The full frozen mechanical gate and integrated before/after performance evidence
were pending when this review was recorded. Targeted application tests validate
orchestration with injected terminal ports; the separate real-child/native tests
provide integration evidence. This review makes no 100% coverage or full runtime
release qualification claim. The record/snapshot work remains bounded by the
protocol cap, and the status mutex is not held across native/provider calls;
latency under the admitted worst-case workloads remains a performance-gate
question rather than a conclusion inferred from code shape.

## Final re-review

PASS retained at source identity
`b5284b7163a2d482efb33d27394327874056c58cffb91ae6596b08d1f47744f8`.
Inspected the corrected worker batching comment: the nonblocking stale example
above is resolved without changing execution. Inspected the additional real
Ghostty view comparison against plain `STATUS_READY`, and the transfer
continuation assertion requiring exactly the original tail bytes. These
assertions exercise observable contracts rather than internal call structure.
`/tmp/pty-program-status-runtime-final.log` records all three integration tests
passing. The earlier independent 14-test application result remains applicable
to unchanged application behavior. No unresolved scoped P1/P2 findings. The
frozen full gate is running separately; this remains a code-quality review
result, not a substitute for that gate.

## Test expansion re-review

PASS retained at source identity
`aaf8eabeb3597197fedffaebba148c631566499d77df1ca81de634df0b62bf8f`.
Reviewed the added application admission/cleanup boundary tests, the domain
control-string opacity test and the test-local counter exhaustion cases. No new
scoped P1/P2 finding. The tests exercise observable refusal, quota rollback,
no-native-feed-before-admission, no partial reply delivery and preserved parser
continuation. The application helpers only widened visibility within the test
module; no production API seam was added.

The two counter tests deliberately synthesize otherwise impractical u64 boundary
states inside `cfg(test)` and then invoke production operations. This is narrowly
scoped arithmetic-boundary evidence, not a claim that public lifecycle workloads
reached exhaustion. The continuation test restores only its synthetic revision
before completing the original partial report, so its final assertion checks that
the rejected scan did not consume the suffix. Control-string cases cover every
two-chunk split for each tested introducer and terminator. Quota tests derive
limits from the public reservation contract and assert resulting state, resource
usage and terminal behavior rather than merely mirroring private call order.

The author reports seven focused domain tests, all 19 application program-status
tests and Clippy passing. This final test-only expansion was inspected without
rerunning those commands; earlier independent behavioral test evidence remains
recorded above. Final full-gate, performance and coverage results remain separate
acceptance evidence.
