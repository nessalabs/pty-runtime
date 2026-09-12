# Verification status

Plain-English snapshot of what we have checked and what is still open.
Product behavior lives in [`usage.md`](usage.md), [`features/`](features/), and
[`adr/`](adr/).

## How we check things

- Run `python3 scripts/gate.py` (format, lint, tests, native build).
- Prefer real process and terminal fixtures over mocks.
- Do **not** commit gate logs or load dumps into the repo. Re-run when needed.
- A green experiment does **not** mean “ready to ship.”

## What we found works

- The library can own real Unix terminal sessions and keep reading them with one
  dedicated reader per live session.
- Detach / reconnect does not kill the child process or invent a fake exit.
- When projection is on, Ghostty drives the terminal model; views show size,
  cursor, text, styles, and modes (including mouse tracking) without collapsing
  them into one vague flag.
- Scrollback is read as a history range. Readers do **not** move a shared
  viewport to peek at old lines.
- Decoders refuse snapshot pages that are too large or badly shaped, so a bad
  page cannot force unbounded memory.
- Idle projected sessions can park to an encrypted on-disk checkpoint and come
  back. Abandoned checkpoint dirs from a crashed owner can be cleaned up within
  finite limits (locks, not PID guessing).
- Helper “guardian” binaries are staged so the parent process does not keep a
  writable executable open (avoids fork/ETXTBSY issues).
- The local demo client can edit, scroll history, paste (including images as a
  temp file path), and resize without the worst of the earlier flicker/paste bugs.
  It builds one `Runtime` on the main thread before starting Tokio, then shares
  it with session workers (helper-image staging must not fork under a live
  multithreaded executor).
- Mechanical gate and a large set of fixtures exist and are the usual proof of
  “this change still works.”

## Clean Code review pass (naming, diagrams, structure)

**Review status: all three specialist reviews completed.** DDD/dependency
direction, Clean Code/SOLID, and adversarial behavioural correctness each ran
independently against the full range. Every P1 and P2 they raised is fixed;
remaining P3s are listed under "What is still open". The behavioural review had
to be run twice — the first attempt died on an API spend limit — and the second
run covered the newer `AdmissionQueue` work that did not exist at the first
attempt.

The reviews were worth running. Between them they rejected one central claim
(see `ProjectionCoordinator` below), found **one real regression** introduced by
the refactor, and caught two cases where a fix for an earlier finding
overcorrected into a new problem:

- `stage_output(b"")` began waking the scheduler and could fail the projection
  from inside the session-context lock. Reachable through the public API and
  through any `IProcessEvents` backend that delivers an empty slice; not
  reachable through the bundled PTY reader, which is why the suite missed it.
  A regression test now covers it, recorded failing before the fix.
- `SourceReaper` was made to pre-reserve the *runtime-wide* storage-slot ceiling
  in *every session* — roughly 512 KB each at default limits, charged against no
  quota — while fixing an "allocation is unbounded" finding. Reverted to growing
  on demand.
- Splitting `max_park_attempts` into a separate `max_delete_attempts` silently
  disconnected a test's retry knob. The test kept passing while pinning nothing.

Two smaller losses of atomicity introduced by the split were also found and
fixed: `read_back_source` had become two lock acquisitions where one was needed,
and `bind_process` could bind onto a projection whose cleanup had completed.

Automated reviewers on the pull request then found three more: the command queue
was being reallocated under the admission lock during cleanup, the encapsulation
claim in this document was overstated (the admitting closure received `&mut
Admission`, so callers could still reach `policy` and the drain fact — it now
receives a narrow `Admitting` view), and a gate test added during this work
proved nothing because it ran the real workspace and only checked stdout. All
three are fixed.

Closing the P3s also turned up one thing worth recording: `SourceReaper::restore_attempts`
is reachable, but not the way it looked. A source given up on *during* a close is
surrendered to the ledger on the next cleanup pass, so closing twice restores
nothing. It only matters when the retries were spent while the projection was
still serving. The first test written for it asserted the wrong scenario and
failed; the scenario, not the code, was wrong. The test that replaced it is
mutation-checked.

Answers to the four questions the behavioural review was scoped to:

- **Lock tiers** — the property they depend on (tier 3 never held across a call
  to an injected port) is true and was verified exhaustively. The *chart* was
  wrong: ten mutexes are reachable, not six, and `StagingLease::drop` takes the
  runtime-wide capacity signal under the per-session admission lock on every
  rejected chunk. Corrected.
- **`SourceReaper` equivalence** — the retry logic is exactly equivalent. Two
  separate defects were found in it (above), neither in the retry arithmetic.
- **Lock tiers are now tested, not just documented.** Every lock a projection
  takes goes through a chokepoint that registers its tier for as long as the
  mutex is held, and acquiring at the same or a shallower tier than one already
  held panics. Eight tests in
  `projection/tests/lock_order.rs` drive worker runs, the park/restore/close
  cycle, teardown, and three concurrent threads through it.

  Three of those tests exist to stop the rest being vacuous. Two assert the
  detector *can* fail by inverting deliberately; the third goes through a real
  leaf lock rather than calling the tracker directly. The others assert which
  nestings actually occurred, because "no inversion detected" proves nothing if
  nothing ever nested.

  The first version of this instrumentation was wrong in two ways an automated
  reviewer on the pull request caught, and the tests as first written could not
  have: `leaf` registered the tier in a local that dropped when the helper
  returned while the caller still held the mutex, so nesting *under* a leaf went
  undetected; and three wiring sites (`wake`, `unbind_process`, and the since
  removed `inject_services`) bypassed the chokepoint entirely, which made the
  every-lock-is-instrumented claim false. Both are fixed, and the third test
  above exists so the first defect cannot return silently.

  Writing them corrected the chart again. The documented
  admission → leaf nesting is real, but not on the path assumed: a chunk
  rejected before its staging lease exists never reaches a leaf. It happens when
  an already-reserved lease is dropped *inside* the admitting closure, which
  notifies the runtime-wide capacity signal while the admission guard is alive.
  The first test written for it asserted the wrong path and found nothing.
- **`finish_io` TOCTOU** — no window. `workspace` is `&mut` under the tier-1
  lock, the blocking job only ever writes the mailbox and never clears it, and a
  panicking job publishes `Err(Worker)` rather than leaving the slot empty.
- **`bind_process` race** — yes, one had been opened. Fixed.

### Naming and diagrams

What changed:

- Domain policy mutators now say that they mutate: `record_activity`,
  `record_processed`, `record_control_applied`, `mark_closed`. The old
  `close()` / `closed()` pair (two different transitions, one letter apart) is
  now `close()` / `mark_closed()`.
- The projection internals were renamed for what they are: `Core` → `Admission`,
  `Engine` → `NativeWorkspace`, `Event` → `Command` (requests going in, as
  opposed to the `TransferEvent` facts coming out), `IoKind` → `PendingIo`,
  `Stored` → `CommittedSource`. The old `garbage` field became `pending_deletes`
  and then moved into `SourceReaper` entirely.
- Coordinator methods that did work behind noun-shaped names were renamed:
  `staging` → `reserve_staging`, `ticket` → `reserve_request_slot`,
  `native_event` → `apply_command`, `pending_native` →
  `poll_inflight_operations`, `finish_stream` → `seal_journal_if_drained`.
- Three ASCII diagrams were added where the rules were previously spread across
  many methods: the `Residency` state chart, the `SessionStatus::completion`
  truth table, and a worked `TransferOrder` cursor example.
- The two-lock split is now documented on `Admission` / `NativeWorkspace`,
  including the actual lock order (**workspace → admission**, taken only by
  `worker::run`, `worker::failed` and `teardown::finish_after_shutdown`). The
  first draft of that comment stated the order backwards; it was checked against
  the code before being committed.

### Structure

- `admit()` replaces the push-drop-wake sequence that was copied into five
  admission paths. It owns the lock guard and drops it before waking, because
  `fail()` re-acquires that same lock — a call site that woke while still
  holding it would have deadlocked.
- The blocking-I/O mailbox is typed per job kind. The job descriptor and the
  result were parallel enums that had to agree by convention, and `finish_io`
  carried a defensive "wrong result kind" arm in every branch. They are now one
  `PendingIo` carrying a `Mailbox<T>` per variant, and all four arms are gone —
  not merely unreachable. Both now live in `inflight.rs` beside the slot they
  fill.
- `worker::run` went from ~100 lines and twelve early returns to 31, with the
  phases named (`read_back_source`, `restore_step`, `serve`). Its two `status()`
  reads were kept: the second one exists to catch a failure that
  `poll_inflight_operations` can introduce without returning a schedule.
- `CompatibilityId` holds the "non-empty, at most 4096 bytes" rule that three
  crates were re-deriving. This also removed a latent defect in teardown, which
  used `unwrap_or_default()` and so recorded unreclaimed sources under an empty —
  and therefore invalid — identity when services had already been released.
- `SessionQuotas` / `InputQuotas` pair each runtime-wide quota with the
  per-session quota it is always charged against, so the two can no longer be
  mispaired at a call site.
- `ControlGeneration` types the ordered resize counter, which was one of three
  distinct counters spelled `generation`.
- `terminal_contract.rs` was split into rendering evidence and
  checkpoint/restore evidence, with shared fixtures, after it crossed the
  350-line limit.

## What is still open

- Full release load goals (large session counts, strict latency budgets, long
  soak) are **not** closed. Short green runs are not a substitute.
- Not every claimed OS/CPU target has a fresh, complete qualification pass on
  current source.
- Restoring a parked session has been signed off against ADR 0003. Each clause
  in "On new output, retain the bytes and restore READY before feeding them into
  the model in order…" was checked against the code and against what the tests
  actually prove:

  | ADR clause | Evidence |
  | --- | --- |
  | Retain bytes, restore READY, feed in order | `ready_interleaves_live_bytes_and_controls_without_releasing_source_before_finish`, and the real-Ghostty `ready_applies_exact_64_byte_suffix_before_history_and_retains_encrypted_source` |
  | Bounded history work between model operations | the `history_step_owed` alternation in `restore_step`, covered by the same tests |
  | Keep the saved source until restoration finishes | `…without_releasing_source_before_finish` |
  | Expose history completeness; inapplicable pages visible | `skipped_history_is_visible_and_lifetime_count_survives_the_next_park` |
  | Never claim full history when only READY completed | `Usable` vs `Resident` residency, same tests |
  | Unrestorable checkpoint → projection unavailable | `malformed_restore_read_fails_pending_view_but_retains_unprocessed_bytes` |
  | …**and process ownership and raw I/O preserved** | `an_unrestorable_checkpoint_leaves_a_real_child_fully_usable` — real `UnixProcessBackend`, real child, real Ghostty, real AEAD and real disk; only the store is substituted so a parked session can be made unrestorable |

  The last row was the real gap and took three attempts to cover honestly.

  The first version asserted that already-buffered replay survived — which it
  does, but those bytes were published *before* the failure, so it proved nothing
  about the claim. The second sent output *after* the failure and required it to
  reach an observer, which was better but still ran against a **mock** process:
  a dummy `IProcessSession` returns a constant pid and succeeds at everything by
  construction, so it would pass whether or not the real child survived. Both
  automated reviewers on PR #3 caught that independently, and `AGENTS.md` is
  explicit that mocks verify orchestration only.

  The clause is now closed by
  `tests/projection_failure_preserves_session.rs`, which runs a real
  `UnixProcessBackend` and a real child and substitutes only the checkpoint
  store. It asserts the same Unix pid is still owned, that input written after
  the failure reaches the child and its echo comes back through the PTY reader,
  that projected observation is refused while raw observation is not, and that
  cancellation still reaps the child. Mutation-checked: cancelling the child on
  projection failure fails the test — which the mock version could not detect.

  The earlier mock-based test remains in `tests/ready_projection.rs` and is
  accurate about what it covers: the orchestration half, that a failed
  projection keeps admitting the reader's output.

  Worth recording separately: two mutation attempts during this work silently
  did nothing because the anchor text did not match, and both looked like
  passes. A mutation check is only evidence once the edit is asserted to have
  applied.
- Memory packing / reclaiming unused native pages after park/restore is not a
  finished production story.
- Shipping packaging extras (notices, examples, docs completeness) still follow
  the ADRs — re-run the notice script when dependencies change.
- **`ProjectionCoordinator` — responsibility moved, size did not.** The first
  attempt boxed three data clumps into `Wiring`/`SessionQuotas`/`InputQuotas`,
  took the type from 16 fields to 7, and claimed the god object was resolved. An
  independent review measured it and rejected that: the method count had gone
  *up*, no responsibility had left the type, and every file still reached every
  field. That was correct and the commit message overstated the result.

  A second pass moved the responsibility. `AdmissionQueue` now owns the queue,
  the domain policy, the drain fact and the cleanup waiters behind one mutex with
  its fields private. The honest measurements:

  | | before | after |
  | --- | --- | --- |
  | `admission.lock()` call sites | 19 | **0** |
  | files reaching into that state directly | 7 | **0** |
  | named operations on the new owner | — | 32 |
  | coordinator methods | 45 | 49 |

  (`journal.rs` and `observation.rs` still lock their own state, as they should
  — they were already independent collaborators.)

  The method count is the point worth being careful about: it did **not** fall,
  because the coordinator's methods are now mostly thin delegation. What changed
  is that they can no longer reach the state — `Admission`'s fields are
  unreachable except through named operations.

  One caveat, raised by an automated reviewer on the pull request and worth
  keeping: `admit` and `admit_output` run caller code under the lock so a
  per-command check and the push are one step. That closure originally received
  `&mut Admission`, which was a hole in exactly the claim above. It now receives
  an `Admitting` view exposing four named operations, so the fields stay
  unreachable — but the closure is still caller-supplied code running under the
  lock, and a future operation added to `Admitting` widens that surface again.

  Whether this means "no longer a
  god object" is a fair argument; what is measurable is that the coupling is
  gone and the invariants that were previously spread across call sites
  (`commit_park`'s atomic emptiness check, `admit_output`'s rejection precedence)
  are now stated in one place.

  Partly addressed, and the distinction matters. The provider-facing **job
  bodies** and the submission primitive were extracted: `read_job`,
  `commit_job`, `delete_job` and `submit` are free functions in `blocking.rs`
  over injected ports, with ten tests that need neither a coordinator nor a
  workspace.

  The **orchestration lifecycle around them** then came out in two pieces and
  stopped. The in-flight slot and the submission are `InFlight` in
  `inflight.rs`, which makes submitting and recording one act — a job cannot be
  running without the slot knowing. The deletion lifecycle is
  `SourceReaper::start_delete` / `finish_delete`, driven without a coordinator in
  `tests/reaper.rs`; those tests pin the distinction between a provider that
  refused (spends an attempt) and a full pool that never asked it (spends none).

  A review of the merged result asked whether removing `commit_park`'s
  `engine_idle` argument let a commit release the model while a resize was in
  flight. It does not, but the reason recorded at the time was wrong. `serve`
  *does* apply queued commands while a commit job is outstanding, so
  `workspace.resize` really can be `Some` when the commit lands; what refuses the
  release is the attempt's `activity` generation, which every admission bumps
  before queueing. `a_resize_admitted_during_a_commit_blocks_the_release` drives
  that interleaving, and deleting the `activity` clause parks the session out
  from under a pending resize.

  `start_read`, `start_park` and `finish_io` remain `ProjectionCoordinator`
  methods in `io.rs`, and they are where the coupling
  actually lives — `finish_io` alone spans five collaborators and ten workspace
  fields, unchanged by the extractions above, because what came out came out
  precisely by touching nothing. That was measured and declined rather than
  deferred; see [`todo/declined.md`](todo/declined.md).

  The native engine driving also remains, and was measured and declined rather
  than deferred — see the P3 list below.

  `Wiring`'s two `#[cfg(test)]` injection seams are gone, closed one PR earlier.
  A fault is wired in before `create` through `Harness::with_services`, and
  scheduler loss through the `IWorkScheduler` double, so no production type
  carries a hook for swapping a collaborator on a live projection. One seam
  remains and is not on a production type: `wiring::hold_leaf` holds its own
  mutex through the real `leaf` helper, because the lock-order test needs a
  closure running while a leaf is held and no production path does that.

  The file layout that review complained about is settled — the "eight files
  partition a method list" finding is closed, not merely reduced.
  `stream_end.rs` held two unrelated methods and is gone: the reader-facing `notify_output_drained`
  sits with the rest of admission, and the worker step that acts on it sits with
  the worker. `completion.rs` merged into `io.rs`, because the two held the start
  and finish halves of one lifecycle and every `PendingIo` variant built in one
  was destructured in the other. `admission.rs` and `teardown.rs` still own no
  type, and were measured and left: every piece of state they touch already
  belongs to one, so a type there would hold a back-reference and nothing else.
  See [`todo/declined.md`](todo/declined.md).

- Left from the three reviews, one item, P3 and not open: the **native engine
  driving** is still coordinator methods, and on measuring it, moving it does
  not look like a win. `apply_command` touches five
  collaborators (`queue`, `journal`, `quotas`, `wiring`, `status`) and four
  workspace fields; `poll_inflight_operations` touches three and four. Passing
  that as a context object to an `impl NativeWorkspace` renames `self` rather
  than splitting a responsibility. Contrast the blocking jobs, which touched
  **zero** collaborators and so came out cleanly. Recorded as considered and
  declined rather than outstanding; it would need a different idea, not more
  of the same one.

- **100% line/function/region coverage** is a stated readiness target in
  `coding_standards.md`. A run now exists: 93.13% lines, 91.75% functions,
  91.13% regions across the workspace, and 12.50% lines for the separately
  instrumented guardian helper. The two permanent holes this record used to
  name are gone — `collect` no longer exists, and `commit_park`'s `engine_idle`
  parameter was removed in favour of a test of the ordering it stood for.

  That did not move the total, because they were never the obstacle. The
  largest single gap, `process/image_materialize.rs`, is **unmeasured rather
  than untested**: every never-executed function in it is the fork child, which
  exits via `_exit` and never flushes its counters, while the parent half of the
  same file records 39 to 61 hits. The guardian helper is the same thing one
  level up. Branch coverage reports 0/0, which is not a pass. The target needs a
  decision about measured scope before a number means anything; the numbers, the
  per-file gap and what is still unreachable are in
  [`todo/release-blockers.md`](todo/release-blockers.md).
- File size is now a **soft** gate: `scripts/gate.py` reports files over 350
  nonblank lines and continues, rather than failing. Its inventory now includes
  `client/`, which had been invisible to it — `client/server/src/wire.rs` (467)
  and `session.rs` (355) were over the line without the gate noticing.


## Where to look next

| Want | Go here |
| --- | --- |
| What is still to do | [`todo/`](todo/README.md) |
| How to embed | [`usage.md`](usage.md) |
| Behavior by area | [`features/`](features/) |
| Why we chose this | [`adr/`](adr/) |
| Early measurements | [`experiments/`](experiments/) |
