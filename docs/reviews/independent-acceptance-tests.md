# Independent loop-4 acceptance-test gap audit

Reviewed and executed 2026-09-08 on macOS 26.6 arm64. Scope was reassigned from
DDD consolidation to the user's requested independent missing-test challenge.
No production behavior was edited. Two `mod` declarations register test-only
modules. The native cursor/corpus and dependency patch are owned by the terminal
reviewer and remain outside this report's final native acceptance scope.

Six new tests pass. That means these six bounded contracts pass, not that the
runtime meets all release requirements or 100% code coverage. No production
defect was confirmed by the new tests. Current implementation already satisfies
their final assertions. Failed fixture assumptions are preserved and explicitly
distinguished from production red/green evidence.

## Identity and executed evidence

Base HEAD is `52ad04c3519616e6cedea9ac8707406970a40ed7` in a dirty concurrent
worktree. [Source manifest](../verification/loop4/independent-acceptance/source-manifest.json)
SHA-256 is `381021ba95e2e32bc23432856a2546e0e38521e79b65d96b981369b86878f14f`.
[Metadata](../verification/loop4/independent-acceptance/metadata.json) records capture
time/platform; the manifest covers core source, process/event adapters, helper
source, manifests, new tests and relevant fixtures. It is not a repository freeze.
[Commands and exit codes](../verification/loop4/independent-acceptance/commands.json)
have adjacent raw stdout/stderr, including the retained initial transfer failure.

| Required command | Actual result |
| --- | --- |
| `cargo test --locked -p pty-runtime-application --lib` | 41 passed, including new cancellation ownership and untimed adapter tests |
| `cargo test --locked -p pty-runtime-infrastructure --test process_failure_isolation -- --nocapture` | 2 passed |
| `cargo test --locked --no-default-features --features event-stream --test event_stream_decode_contract -- --nocapture` | 2 passed |
| `cargo clippy --locked -p pty-runtime-application --all-targets -- -D warnings` | Passed |
| `cargo clippy --locked -p pty-runtime-infrastructure --test process_failure_isolation -- -D warnings` | Passed |
| `cargo clippy --locked --no-default-features --features event-stream --test event_stream_decode_contract -- -D warnings` | Passed |

The coordinator owns the required final `python3 scripts/gate.py` run. Its current
core tests, all-target/all-feature workspace tests and event-only tests select
these additions automatically; no ignored test or manual-only acceptance path
was added. The prior macOS gate attempt 2 predates these tests and cannot serve
as their final consolidated gate result. Linux execution was not performed here.

## Existing behavior map and additions

The audit read ADR 0001–0005 requirements and the proof ledger, then inspected
current tests before adding coverage. The older `release-gap-audit.md` includes
superseded READY and diagnostics findings; it is a gap inventory, not a current
assertion that those implementations are absent.

| Intended behavior / relevant requirement | Existing concrete coverage | Missing combination addressed or remaining limit |
| --- | --- | --- |
| Helper loss preserves truthful status and cleanup; unrelated session remains serviceable (G1-04/09/14) | `process_guardian_failures` checks sentinel/guardian loss and unknown workload exit; `process_foreground_switch` checks known foreground members and outside survivor for cancellation | New `process_failure_isolation` independently SIGKILLs S or G, verifies explicit failure/drain, inventories the known victim shell and child until neither is live, then successfully writes/reads/resizes a second admitted PTY before backend shutdown. Guardian loss must not invent W exit. Zombie/reaped distinctions remain explicit: inventory proves no live fixture workload, not host-wide zombie absence. |
| Cancellation cannot detach admitted checkpoint memory from its quota; close drains late result (G3-04/11/12/13) | `transfer` covers cancellation before execution, independent pins/observers and rejected executor submission; `cleanup` covers close during commit and failed deletion | New `acceptance::cancelled_parked_transfer_keeps_inflight_memory_charged_until_close_drains_read` holds the accepted parked read at the existing executor barrier, cancels its consumer, starts close, verifies memory/source stay owned, then releases the read and requires source deletion and all relevant reservations released. A completed close waiter remains charged until explicitly dropped. This is orchestration proof with existing fake ports, not real disk/native evidence. |
| READY/history validation precedes successful End; skipped history is truthful (G2-08/G3-07/09) | `ready` checks live output/resize alternation, source ownership, skipped cumulative counts and late corruption/failed End; `terminal_live_restore` and `ready_projection` cover real engine/provider paths | No duplicate test added. Native row/cursor divergence work still needs its own final verified patch, real-native regressions and source-aligned execution. |
| Transfer ordering/loss/wakers stay correct (G3-13) | `transfer`, `ordered_review`, `ordered_transfer_runtime`: independent byte/control cursors, eviction/resync, consumer-held quotas, reentrant wake disposal, immutable failed End and two real replicas | New cancellation/close combination above adds ownership coverage. Many-consumer real journal-loss/resnapshot and full load remain acceptance work. |
| External record conversion rejects malformed input before domain admission (A-05/G4-12) | `event_stream_failures` checks sink rejection, uncertain commit, cancelled append and contradictory receipts; `event_stream_reconnect` checks legitimate gap/bytes/completion and foreign schema | New public `event_stream_decode_contract` obtains real publisher/store gap, byte and completion records. It rejects every prefix truncation, appended byte, over-cap payload, schema/version/cursor mismatch, bad magic/tag, empty/backward/inconsistent ranges, unknown completion codes/bool, incomplete completion, and completion advancing byte position. Valid originals are independently checked first. |
| Missing timing is distinct from failed or successful timing (G4-03/15, replacement boundary A-04) | Existing histogram boundary/reset test and four public real-PTY diagnostics tests; poisoned-context test checks ownership gauges | New `diagnostics::tests::untimed_replacement_adapter_reports_unavailable_without_false_success_or_failure` invokes the actual default `IProcessSession::resize_timed` method on an uninstrumented replacement. Resize succeeds once; metrics have one unavailable and zero success/failure/max. Abandoning a real Timing separately records failure, never a success sample. |
| Crash cleanup cannot scan past caps, follow foreign names or erase live sources (G3-12) | Current `checkpoint_crash_cleanup`: actual child death, live locks, byte/namespace budget, hidden-prefix starvation, unsafe names/links/modes, initialization race, inherited maintenance lock; provider/adversarial suites cover immutable objects and replacement directories | No duplicate test added. Injected filesystem iterator/read/unlink failures and full restart populations remain below. |
| Raw identity, cancellation and drain are separate (G1-03/04/05/07/10) | `raw_registration`, `raw_adversarial`, `raw_completion`, `input_reservation`, `process_contract` and `process_adversarial` | Guardian isolation adds a new actual fault combination. It does not replace exact full lifecycle counts or acquisition-edge injection. |

## Test-development failures and TDD interpretation

[Preserved fixture failures](../verification/loop4/independent-acceptance/initial-fixture-failures.md)
explain the initial LF/CRLF marker, legal truncated drain, event-store page-budget,
and retained close-waiter assertions. The substantive requirements were not
relaxed: victim cleanup, independent-session responsiveness, no invented exit,
malformed record rejection, in-flight reservations and post-Drop release all
remain required. No production change was made to make these tests pass.

The user's requested TDD approach applies when a desired behavior is absent or
incorrect: keep its failing regression before changing production and re-run
unchanged assertions afterward. Existing correct behavior can pass a newly added
test on its first valid run. This report does not manufacture a red phase by
calling fixture bugs product bugs or claim mutation testing was performed.

## Remaining required proof and fault-injection seams

The target remains **100% measured coverage of relevant production code**, with
no exclusions added merely to raise the percentage. This review did not run a
coverage tool and cannot assert any measured percentage. The coordinator is
setting up that measurement. Line coverage alone does not establish all outcomes
or interleavings; the following paths still need explicit tests/controlled seams
when coverage and contract review show they are unexercised:

* `process/{image,spawn,registration,supervisor}.rs` and helper startup: failure at
  each executable materialization, channel/descriptor acquisition, helper exec,
  reader spawn and registration edge, with baseline FD/child/worker restoration
  and an unaffected admitted session. Existing failed-spawn tests do not establish
  every edge. Do not rely on nondeterministic host-wide resource exhaustion.
* Helper discovery/anchor retirement: error, truncated enumeration and fork
  admission failure must retain recovery ownership; helper-group entrants,
  abrupt runtime-owner death and partial protocol writes must be executed through
  the packaged runtime path on each supported OS. Finite shell fixtures are not
  proof against unbounded forking/group migration or simultaneous helper loss.
* `checkpoint/{inventory,filesystem,cleanup}.rs`: deterministic iterator errors,
  short/interrupted I/O, mid-pass unlink failure, and worker completion after
  cancellation need source-retention/error/partial-report assertions; do not
  interpret inaccessible or unexamined entries as reclaimed/free capacity.
* Event codec status conversion needs a complete valid/invalid code matrix when
  coverage identifies missed variants; allocator failure before encoding and
  all contradictory receipt identities must preserve pending source ownership.
  Current malformed record tests do not simulate allocation failure or a complete
  external-store fault model.
* Full real-provider/live-PTY combinations of corrupt parked state, pending
  controls, bounded parser backlog and still-operational cancel/exit; repeated
  multi-consumer resnapshot after journal loss; every intended native state
  transition. The terminal reviewer owns the discovered native divergence.

All six additions must remain green in final gate runs before the scoped loop can
be called ready. Full release additionally requires the unchanged ADR workload:
64 resident/16 barrier-started producers at 10 MiB/s, 1/32/128 resource sweeps,
at least five 60-second runs per performance case with prescribed percentiles,
64-idle CPU measurement, actual 10,000 lifecycle cycles, 100,000 attach/detach
operations, and a full 12-hour meaningful mixed soak. Record seeds, failures,
resource plateaus, native/helper identities and current platform executions.
No bounded test count here substitutes for those criteria or a reviewed physical
reclamation/guardian resource policy.


## Coverage-matrix follow-up: projection I/O and resize boundaries

The completed `docs/verification/coverage-matrix-1/workspace.json.gz` identified
unexecuted application branches in `projection/io.rs` (read validation and
rejected transfer admission), `completion.rs` (panicking read classification),
and `native.rs` (OS resize admission/completion errors). These are behavioral
boundaries, rather than accessor targets or artificial denominator exclusions.
Six additional tests now exercise them in `tests/io_faults.rs` (five tests,
including two opened-checkpoint cases) and `tests/native_faults.rs` (one test
covering both admission and asynchronous completion failure).

- A shorter-than-advertised stored ciphertext returns exactly
  `Storage(Unavailable)` to a parked transfer, leaves the session Parked and its
  source charged, and releases transient read/request/observer leases.
- The same malformed read during restoration fails an already admitted view
  with that exact error, keeps the original processed watermark and five queued
  parser bytes, creates no native model, and releases staging on close.
- A protector returning the wrong descriptor or an oversized plaintext capacity
  returns exactly `InvalidConfiguration`, without deleting the sole source.
- A panicking store read becomes `Worker` for its transfer; permits are released
  and a later healthy read returns the original checkpoint bytes.
- Rejected blocking transfer admission returns `Capacity`, queues no job,
  releases temporary permits, and allows a fresh successful read.
- OS resize rejection and completion failure both return explicit OS/model
  `Process(Io)` outcomes, retain the old model dimensions and generation, release
  request/staging permits, and allow a successful retry at generation one.

Only test modules and deterministic provider/process fault controls changed.
The first executions passed; there is no claimed production red/green defect.
The full application unit suite passes **52 tests**, and application all-target
Clippy passes with warnings denied. Logs and production/test hashes are in
`docs/verification/projection-io-faults/`. All changed test files remain below
350 nonblank lines. These additions are not included in the already completed
coverage-matrix-1 percentages; a later frozen run must measure their effect.
The coordinator owns final gate/independent-review acceptance. Native corpus
follow-up and remote fixture mutation were not part of this scoped work.
