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

**Review status: incomplete.** `AGENTS.md` requires three independent specialist
reviews per loop. Two ran to completion — DDD/dependency-direction and Clean
Code/SOLID — and their findings are reflected below and in "What is still open".
**The third, adversarial behavioural correctness, did not run: it terminated on
an API spend limit.** Nothing in this pass has had an independent concurrency,
resource-bound, cleanup or failure-injection review. The mechanical gate is
green and the suite is stable over five consecutive full runs, but neither is a
substitute, and this loop must not be called passed until that review is done.

A Clean Code review of the domain and application core, applied in verified
steps. **No behaviour changed at any step**: every pre-existing test passes with
no test removed, and the mechanical gate is green after each commit. Test files
were touched for identifier and type changes only — no assertion, condition, or
expected value was altered. Where an assertion had to change shape, the old and
new forms were shown to compare the same value.

One measurement caveat worth recording: the first steps were verified with
`cargo test --workspace`, which does **not** build the `event-stream` targets.
The gate always ran the full `--all-features` matrix and stayed green, and
verification now uses `--all-features` too.

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
- **The `ProjectionCoordinator` god object is NOT resolved.** A later pass
  extracted `SourceReaper` and `Wiring`, taking the type from 16 fields to 7.
  An independent review measured what actually changed and the answer is: not
  enough. Field count fell because three data clumps were boxed; **the method
  count went up** (45 → 50 across the nine files holding its `impl` blocks), no
  responsibility left the type, and all nine files still reach every field. The
  commit message for that change ("Two concerns … now own their own state")
  overstated it, and this record previously repeated the overstatement.

  What would actually meet the claim is moving *methods*, not fields: an
  `AdmissionQueue` owning the queue mutex and its quotas, a `BlockingIo` owning
  the in-flight job and the executor, an `impl` on `NativeWorkspace` for engine
  driving, and a teardown owner. `SourceReaper` is the model — it is the one
  piece of this work that genuinely took a responsibility away. That remaining
  split changes locking and needs its own review loop.
- `ITerminalFactory::compatibility` still returns `&'static str`, so the
  application converts it to `CompatibilityId` rather than the adapter doing so
  at its own boundary. Two independent reviews flagged this as the wrong side of
  the port; `ITerminal::resize` was widened to `ControlGeneration` in the same
  work, so the two newtypes are treated inconsistently.
- `max_park_attempts` is a domain budget documented for parking retries, but it
  is also used as the checkpoint-deletion retry bound. Tuning one silently
  retunes the other, and the domain models only one of them.
- `stage_output_observed` still reimplements the enqueue sequence inline instead
  of using `admit()`, and holds the admission lock across staging reservation
  and the payload copy.
- Building an `UnreclaimedSource` from a park attempt is duplicated verbatim in
  two files, and the unreclaimed ledger is a bare `Mutex<Vec<_>>` with no owner.
- `ControlGeneration::from_raw` is public with no production caller; every use is
  a test. Its Rustdoc justifies it by a deserialization contract that does not
  exist.
- The lock-order note added to `Admission` documents two mutexes, but six are
  reachable from a coordinator. The three inside `Wiring` are leaf locks today —
  load-bearing and undocumented.
- Files under `client/` are outside `scripts/gate.py`'s size inventory, so
  `client/server/src/wire.rs` (467 nonblank) and `session.rs` (355) exceed the
  350-line rule without the gate noticing.

## Where to look next

| Want | Go here |
| --- | --- |
| How to embed | [`usage.md`](usage.md) |
| Behavior by area | [`features/`](features/) |
| Why we chose this | [`adr/`](adr/) |
| Early measurements | [`experiments/`](experiments/) |
