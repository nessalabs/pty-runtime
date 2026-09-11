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

Answers to the four questions the behavioural review was scoped to:

- **Lock tiers** — the property they depend on (tier 3 never held across a call
  to an injected port) is true and was verified exhaustively. The *chart* was
  wrong: ten mutexes are reachable, not six, and `StagingLease::drop` takes the
  runtime-wide capacity signal under the per-session admission lock on every
  rejected chunk. Corrected.
- **`SourceReaper` equivalence** — the retry logic is exactly equivalent. Two
  separate defects were found in it (above), neither in the retry arithmetic.
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
  not merely unreachable.
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
- Restoring a parked session still has careful rules around “ready” vs finishing
  history; that story still needs a clean sign-off against the ADRs.
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

  Still worth doing, and not yet done: the blocking-I/O lifecycle
  (`submit_io`/`start_*`/`finish_io`) and the native engine driving
  (`apply_command`/`native_call`/`poll_inflight_operations`) are still coordinator
  methods rather than types that own their state.

- Still open after the three reviews, all P3:
  - `SourceReaper::restore_attempts` (the second-close retry round) has no test;
    reaching it needs a close landing after delete attempts have been burned.
  - `commit_park`'s engine-idle half is untested — a commit landing with an empty
    queue but an in-flight reply or resize. The review confirmed the read is safe
    (those fields are workspace-owned), but no test pins it.
  - Nothing tests the lock tiers. There is no lock-order assertion and no
    loom/shuttle model; `close_race.rs` covers one specific interleaving with a
    real thread and everything else is single-threaded pumping.
  - `max_delete_attempts` is only exercised at its default and at 1; its
    validation bounds (0, >100) have no test.
  - `collect`'s empty-mailbox fallback is now unreachable, so it is a permanent
    region-coverage hole.
  - `request_close` and `abandon_close` allocate a `Vec` under the admission
    lock where the old code returned the `VecDeque` itself.
  - The blocking-I/O lifecycle and the native engine driving are still
    coordinator methods rather than types owning their state — the same
    treatment `AdmissionQueue` received.
- File size is now a **soft** gate: `scripts/gate.py` reports files over 350
  nonblank lines and continues, rather than failing. Its inventory now includes
  `client/`, which had been invisible to it — `client/server/src/wire.rs` (467)
  and `session.rs` (355) were over the line without the gate noticing.


## Where to look next

| Want | Go here |
| --- | --- |
| How to embed | [`usage.md`](usage.md) |
| Behavior by area | [`features/`](features/) |
| Why we chose this | [`adr/`](adr/) |
| Early measurements | [`experiments/`](experiments/) |
