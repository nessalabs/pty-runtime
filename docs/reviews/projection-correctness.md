# Independent projection and runtime correctness review

2026-09-08; implementation snapshot based on `cc69ab47aeb73d717a702edeb14540605463e743`
plus concurrent uncommitted projection/runtime implementation. This is an
adversarial correctness review, not the separate DDD or Clean Code/SOLID review.
No production files were edited by this reviewer.

Current outcome: the two P1 correctness findings, process-descriptor P2, and
domain-transition P2 below were fixed and independently re-reviewed as resolved.
No remaining blocker was found in this reviewed scope. G2/G3 and the full gate
remain open for the separately listed missing qualification and observer work.

## Findings and independent resolution

### Resolved P1: a process future panic permanently removes the cleanup executor

`crates/application/src/projection/native.rs::pending_native` polls injected
process futures without containment. `crates/infrastructure/src/scheduling/pool.rs`
catches a panic, invokes `failed()`, and returns `WorkSchedule::Finished`, deleting
the registration. `ProjectionCoordinator::failed` records failure and releases
pending native operations but does not close the model or delete its sources.
`crates/application/src/runtime/projected.rs::close` ignores the failed close
wake and repeatedly waits until Closed. The registration can never run again.
Thus ordinary runtime shutdown/drop hangs after this contained worker panic.

Independent regression: `tests/projection_correctness.rs::process_future_panic_does_not_strand_runtime_shutdown`.
It uses the real standard scheduler, a fake process whose admitted resize future
panics, injected terminal/provider fixtures, and the public runtime facade.
`cargo test --test projection_correctness -- --nocapture` reproduced failure
within the explicit two-second shutdown deadline. No real child process is used.
The failed test's blocked threads terminate with the test executable.

Required fix: preserve a usable cleanup path after worker failure, or contain
external-operation failure before the scheduler retires the registration. All
queued waits must resolve; wake rejection must not strand an admitted operation.
Simply setting Failed does not make source ownership or runtime close complete.

Independently re-reviewed as resolved: scheduler failure callbacks now return a
schedule; the coordinator retains its registration, discards the uncertain
operation without repolling, and schedules another cleanup step when already
Closing. The new `resize_panic_during_close_keeps_cleanup_scheduled` regression
challenges a second-poll panic specifically during close. All four tests in
`projection_correctness` pass after these changes. Shutdown also has a fallback
after both worker pools have joined, retaining uncertain sources conservatively.
Re-review must exercise actual scheduler behavior and verify Closed with retained
old session handles, rather than manually invoking coordinator work after panic.

### Resolved P1: permanently impossible global capacity is accepted at spawn

`crates/application/src/runtime/owner.rs::spawn` checks a process read chunk
against per-session feed/staging limits but not global projection staging
capacity. A valid global `staging_bytes = 1` admits a projected session using
an 8,192-byte read chunk. The coordinator returns transient backpressure forever
for a larger chunk because no quota release can make that chunk fit.

Likewise, every parser feed first reserves `terminal.reply_bytes` from the global
view pool, even for output with no reply. A view pool smaller than that fixed
reservation cannot make progress. The retry uses a five-millisecond timer and
never becomes a typed permanent failure. Check authoritative-reply size against
global input capacity as well; larger-than-total input cannot become writable
through retry. Check required checkpoint/protection reservations for the same
permanent-versus-transient distinction.

Independent regression: `tests/projection_correctness.rs::impossible_global_parser_budgets_reject_before_child_launch`.
`cargo test --test projection_correctness impossible_global -- --nocapture`
fails: the backend was called rather than rejecting the incompatible global
staging capacity. The second table case covers the insufficient reply pool.
Required fix: validate indispensable fixed reservations against runtime limits
before creating native state or launching a child, with a typed outcome. Finite
contention between otherwise admissible sessions is a different case and may
legitimately wait for capacity.

Independently re-reviewed as resolved: runtime spawn now checks global staging,
reply/view, and reply/input constraints before child launch. Both regression
table cases pass; native ownership inventory slots are reserved before creation.

## Verified and supported behavior

The added regression
`shutdown_closes_model_with_completed_request_wait_retained` passes against the
public facade and real scheduler. It retains the only completed request wait,
checks another request is rejected, then observes shutdown reach Closed while
the copied view and old session handle remain readable. Close does not require
ordinary request capacity.

Reviewed existing coordinator tests cover output during encoding and commit,
stale-source deletion, source-preserving restore/authentication failure,
partial authoritative writes without resend, one authoritative query reply,
full request capacity during close, close during commit, finite delete failure
with a retained charged ledger, and service/handle release on Closed. These
provide focused orchestration evidence. They do not replace actual provider,
terminal, transport, or platform qualification.

The restoration policy guards `Failed`, `Closing`, and `Closed` against a late
residency publication, avoiding the apparent close-during-restore resurrection
race. Parking commit acceptance also checks generation, activity, processed
position, successful control generation, and pending work before releasing the
live model. Raw publication occurs only after projection admission, under the
context lock; parser staging is independent of evictable replay storage.

Runtime admission retains an in-flight spawn guard; shutdown closes admission
before backend shutdown and waits for admitted spawns before projection-service
release. Correctness depends on the documented backend contract rejecting spawns
after shutdown and owning/reaping partial spawn failures. Forgotten identities
are removed with lifetime comparison; old handles retain their old context.

## Additional author-coordinated checks

The coordinator author acknowledged unchecked committed reference key/length and
hardcoded protection overhead and is changing those boundaries. Successful
commits must not release the model using a reference for another generation or
an oversized object. An invalid returned reference or a provider panic after a
side effect needs conservative ownership/accounting, not deletion of an
unverified unrelated reference. Source review confirms validation before model release and conservative unknown
source ledger entries on an uncertain commit. Those entries retain expected
identity and quota without inventing a provider reference. Author unit tests
cover panic after publication and invalid references; that evidence remains
distinct from the independent public-facade tests.

Successful parking must reset the failed-attempt counter; the author independently
found the observation-only restore/repark exhaustion case and is adding a proof.
The pure policy now resets successful parking attempts; this was inspected in
the updated source and remains backed by the author’s domain regression.

## Commands and scoped performance evidence

- `cargo test --test projection_correctness`: first panic/shutdown regression
  failed as described above; this is intentionally retained as a blocking proof.
- `cargo test --test projection_correctness impossible_global -- --nocapture`:
  failed the admission regression before fixes.
- `/usr/bin/time -l cargo test --test projection_correctness shutdown_closes_model -- --nocapture`:
  passed one test; raw output in `work/projection-correctness-performance.txt`.
  Reported 0.95 s wall time and 201,637,888 bytes maximum RSS include Cargo and
  compilation. These are scoped test-command measurements, not runtime/session
  resource claims or the release performance requirement.
- `python3 scripts/gate.py`: failed at workspace formatting during concurrent
  implementation; raw output in `work/projection-correctness-gate.txt`.

Platform: Darwin 25.6.0, arm64, XNU 12377.161.13~4. These checks were not run on
Linux in this review. A clean gate and regression rerun after fixes are required.

G2/G3 remain open. In particular, the ordered observer event stream and atomic
snapshot-to-live transfer are not yet wired. Real terminal/provider failure
qualification, lifecycle repetitions, scoped runtime performance, full release
performance and the required soak remain separate proof obligations. Passing
these focused tests cannot close those gates.


## Resolved P2 discovered through resource measurement

Initial release measurements showed 6 open descriptors after shutdown at one
session and 21 at sixteen, although all descendants had exited. Source inspection
found nonoptional reader-wake sockets retained by old process handles. The new
`tests/process_handle_resources.rs` reproduced 21 descriptors versus a four-FD
baseline after completing/forgetting sixteen sessions and dropping the runtime.

The process adapter now detaches wake endpoints after reader join. Independent
rerun passes with all sixteen old public handles retained. Updated release
measurements show five descriptors after shutdown at both counts (the runtime
owner itself remains held during this sample) and zero descendants. Dropping the
runtime in the dedicated regression returns to the exact four-FD baseline.

## Scoped real projected PTY performance fixture

Added `tests/runtime_performance.rs` (ignored except for explicit release runs),
`tests/fixtures/performance_support.rs`, fixture dispatch, and
`scripts/performance.py`. Each run admits all producers and receives all readiness
records before releasing any producer. One and sixteen independent real PTYs each
emit four MiB of deterministic printable bytes, then a unique cleared-screen
suffix. Independent observers validate every raw byte; the test requires exact
published/processed counts, correct native suffix views, no projection failures,
actual successful child exits, EOF drain, and Closed projection state.

The script writes timestamped unique raw/JSON evidence, rejects an existing
output directory, preserves timeout output and metadata, terminates Cargo and
its test process together on timeout, and rejects a source digest change during
the run. Two unit tests verify timeout preservation and refusal to overwrite.

Fresh stable-source evidence: `work/runtime-performance/20260908T092520Z-361c1d0f`.
One-session payload throughput was 78,417,872 bytes/s; sixteen-session aggregate
throughput was 117,126,800 bytes/s. Exact validated raw and processed totals were
4,194,326 and 67,109,222 respectively (including readiness and suffix records).
After shutdown both cases had five sampled descriptors and zero descendants.
The source digest, platform, build identity, startup/release skew, elapsed time,
cleanup time, samples, and configured budgets are in the machine-readable files.

These are one short correctness-validated trial at each count, with observation
and byte-validation overhead included. RSS is sampled only in the test process;
helper/workload memory is excluded. Peak RSS and PSS remain unavailable/null.
No threshold or baseline is invented, and these are not the five 60-second G4
runs, capacity-scale memory qualification, or the full release soak.

## Independent DDD review of coordinator scope

The root requested this additional scope because the coordinator author cannot
independently review its own policy placement. Reviewed domain projection policy,
options and outcome models; application admission, worker, native/IO completion,
lease accounting, teardown and ownership state; and the external terminal,
process, scheduler, store and protector ports. This supplements, rather than
replaces, the other specialist's root-wiring review.

Found and resolved P2: the public `ProjectionPolicy::residency(Residency)` setter
allowed an arbitrary jump to Parked/Closed, bypassing the domain's parking commit
and restoration rules. The runtime call sites were constrained, but the domain
API did not enforce its own transition contract. It has been replaced by
`begin_restore()` (only Parked) and `restoration_progress()` (only Restoring or
Usable). Invalid sources produce typed errors; Closing/Closed and failures are
preserved. Independent source re-review confirmed all application call sites now
submit these facts and handle their errors. The six domain projection tests pass,
including invalid-source and late-completion coverage.

No remaining DDD blocker found in this coordinator scope. Domain owns idle and
retry eligibility, exact positions, commit validity, failure/residency facts,
validated bounds, and portable outcomes. Application owns resource leases,
queues, in-flight external operations, and the lifetime of actual native/provider
objects. Those are use-case resource ownership, not duplicate domain aggregates.
The concrete lease types belong in application because they release actual
resources and notify a capacity port. Protection size is requested through the
replaceable protector port; algorithm-specific overhead no longer enters the
coordinator. Provider/native DTO or OS implementations do not enter domain or
application dependencies.

Independent rerun after transition and accounting changes: six domain projection
tests and twenty-one application projection tests passed. The four independent
public-facade regressions and real process descriptor regression were also rerun.
Scoped Clippy on the new Rust test targets passed. The mandatory gate rerun still
stopped at workspace formatting during parallel implementation, recorded in
`work/projection-correctness-gate-rereview.txt`; no full gate pass is claimed here.

Latest performance fixture additionally reserves enough finite raw replay for
all offered bytes, so legitimate observer descheduling cannot create an
incidental replay gap in this correctness-validated measurement. Latest stable
raw evidence is `work/runtime-performance/20260908T092939Z-8835d139`: approximately
80.2 MB/s at one session and 92.3 MB/s aggregate at sixteen; replay budgets are
4,198,400 and 67,174,400 bytes respectively. Preserve earlier records as earlier
configurations; do not combine them into repeat statistics or a regression claim.
