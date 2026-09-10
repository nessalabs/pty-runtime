# Loop 2 follow-up adversarial correctness review

Reviewed 2026-09-08 against current `coding_standards.md`: checkpoint adapter
fixes, application scheduling ports, fixed work scheduler, blocking executor,
shared worker ownership and capacity signal. Tests/docs only changed by reviewer.
Full G1/G2/G3 and release qualification remain pending.

## Checkpoint findings re-reviewed

`cargo test --locked -p pty-runtime-infrastructure --test checkpoint_adversarial`
now passes all three independent tests on macOS arm64. In particular, renaming
the owned directory then placing a foreign empty replacement at its saved path
no longer lets Drop remove the replacement. Code now anchors the parent and
checks original directory identity before namespace unlink. That resolves the
reproduced pre-Drop replacement defect. A trusted parent remains an explicit
contract: an adversary concurrently renaming after an inode check cannot be
excluded by a portable check-then-unlink sequence. No unconditional hostile
same-UID-parent safety claim is supported.

`CheckpointProtector::protect` now takes plaintext into a zeroizing owner before
any fallible validation/entropy return and zeroizes spare capacity before
in-place encryption and potential growth. Re-read these operations in order;
they resolve the two identified plaintext-memory hardening gaps at this boundary.
This is code-based verification of error-path ownership, not an assertion that
all arbitrary caller copies or prior allocator history can be scrubbed.

The earlier detailed crypto/storage review and its original failures remain in
[checkpoint-adapter-correctness.md](checkpoint-adapter-correctness.md). Default
runtime storage, parking generations, pins and restore races are still pending.

## Scheduler findings and independent reproductions

| Priority / status | Evidence | Required result |
| --- | --- | --- |
| P1, reproduced; fix pending re-review | `scheduling/pool.rs::run`, `drop(work)` outside panic containment | Last strong callback-owner destructor may panic after `run` returns. Worker dies before clearing running state; with one worker, unrelated accepted registrations never run. Retire failed registration and keep worker service intact even when arbitrary owner destruction panics. |
| P2, reproduced; fix pending re-review | `scheduling/pool.rs::with_workers`, infallible slots collection; `blocking.rs::new`, VecDeque::with_capacity | Both accept usize::MAX capacity and panic with allocation overflow. Expected configuration/admission failures require typed Capacity, not panic. Fallible reservation must happen before threads are started; worker count itself also needs a reviewed upper admission bound. |
| P2 documentation correction | Application `IWorkScheduler::shutdown` / `IBlockingExecutor::shutdown` versus `workers.rs::join` | Worker-thread shutdown intentionally returns before joining itself or other workers. Public contract currently unconditionally promises joining. State the exception and require an external owner call to wait for worker completion when such a caller exists. |
| P3 contract clarification | `IWorkHandle::wake` and terminal callback result | Wake during running Dormant/After work is preserved; Finished or concurrent close removes the registration even if wake returned Ok. Document that terminal completion/close supersedes wake, so integrations do not treat Ok as an unconditional future callback guarantee. |

Added `crates/infrastructure/tests/scheduling_adversarial.rs` and executed
`cargo test --locked -p pty-runtime-infrastructure --test scheduling_adversarial`.
All three tests failed on macOS arm64 before fixes: two caught capacity-overflow
panics; the destructor-failure test waited two seconds for a separately registered
healthy callback, then shut down the pool before asserting lack of progress.
No orphan worker or hung fixture is left by the failing test. The original
failure/timeout remains recorded rather than omitted from the proof.

## Boundedness and wake/cleanup review

Work registration stores a fixed slot, weak callback reference and generation,
coalesced pending bit, and at most one deadline. Slot generation checks prevent
old handle close/wake from affecting a replacement. Taking work, clearing pending
and marking running occur under one state mutex; callback execution and callback
owner destruction occur outside it. A wake arriving while callback runs sets a
pending bit retained across Dormant/After completion. Running registrations are
not reused until completion, preventing same-slot overlap across worker threads.
Round-robin cursor avoids a continuously woken slot always winning the scan.
These are inspected invariants; mixed integrated PTY fairness/latency is not
measured by this review.

Capacity signal generation changes under the same mutex used by wait_after.
Notification before wait is observable by its generation mismatch; spurious
wakeups recheck the predicate. Generation wrap is technically possible after
2^64 notifications, so the helper is not a durable monotonic ID allocator.
Deadline waits return to caller predicate checks. The system clock uses elapsed
monotonic time; injected clock policy must still be wired through runtime parking.

Blocking admission counts queued plus executing jobs under one lock. Work is
popped and active count raised atomically, and panic containment wraps executing
FnOnce jobs. Shutdown closes admission and drains accepted finite jobs, unlike
work-scheduler shutdown which discards dormant/pending registrations and finishes
in-flight units. Callback providers must finish: neither adapter can safely
preempt an arbitrary stuck native/storage function. Queue count does not bound
arbitrarily large closure captures; runtime byte/pin admission must precede boxing
jobs and be retained by each actual queued/running operation.

Worker JoinHandles live outside shared worker state, avoiding an owner/worker Arc
cycle. External concurrent joins coordinate through a joining flag and condvar;
a worker calling shutdown returns so an external join cannot deadlock waiting
for that callback. If the final scheduler owner is dropped on its own worker,
remaining handles detach and shared state lives until loops exit; synchronous
post-Drop join cannot be promised in that exceptional case. Tests that shut down
from outside and compare strong counts prove their specific cleanup path, not
full runtime cleanup under every callback/destructor interleaving.

The mechanical gate must rerun after fixes and independent regression additions.
No scheduler adapter success alone satisfies parser staging, encrypted parking,
128 active projected sessions, platform resource limits, or the 12-hour soak.

## Final fix re-review

Re-read the changed scheduler constructors and run loop, then independently ran
`cargo test --locked -p pty-runtime-infrastructure --test scheduling_adversarial
--test scheduling_pool --test scheduling_blocking`: all eleven tests passed.
Added and ran one further independent excessive-worker test, so
`scheduling_adversarial` now passes four tests, including worker counts 65 and
usize::MAX rejected before thread creation.

The constructors use fallible reservation before initializing slot/queue storage
or starting workers and enforce one to 64 workers. Final callback-owner Drop is
now contained outside the mutex; destructor panic retires the registration and
the next healthy work proceeds on the same pool. The reproduced P1/P2 scheduler
findings are resolved in this adapter scope. Original failures remain above.

Application port Rustdoc now states the self-worker shutdown exception and
requires external shutdown for join completion; it also explicitly states that
close/Finished supersedes concurrently accepted wake. Those documentation
findings are resolved. No remaining blocker was found in this review's adapter
scope. This does not mark any full ADR milestone complete or validate integrated
PTY/native/storage scheduling, platform latency, resource defaults or soak.
