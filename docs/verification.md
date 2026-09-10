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

## Readability pass (naming and diagrams)

A Clean Code review of the domain and application core produced a rename and
documentation pass. **No behaviour changed**: the same 231 workspace tests pass
before and after, with an identical test-name set, and the mechanical gate is
green. Test files were touched for identifier renames only — no assertion,
condition, or expected value was altered.

What changed:

- Domain policy mutators now say that they mutate: `record_activity`,
  `record_processed`, `record_control_applied`, `mark_closed`. The old
  `close()` / `closed()` pair (two different transitions, one letter apart) is
  now `close()` / `mark_closed()`.
- The projection internals were renamed for what they are: `Core` → `Admission`,
  `Engine` → `NativeWorkspace`, `Event` → `Command` (requests going in, as
  opposed to the `TransferEvent` facts coming out), `IoKind` → `BlockingJob`,
  `Stored` → `CommittedSource`, `garbage` → `pending_deletes`.
- Coordinator methods that did work behind noun-shaped names were renamed:
  `staging` → `reserve_staging`, `ticket` → `reserve_request_slot`,
  `native_event` → `apply_command`, `pending_native` →
  `poll_inflight_operations`, `finish_stream` → `seal_journal_if_drained`.
- Three ASCII diagrams were added where the rules were previously spread across
  many methods: the `Residency` state chart, the `SessionStatus::completion`
  truth table, and a worked `TransferOrder` cursor example.
- The two-lock split is now documented on `Admission` / `NativeWorkspace`,
  including the actual lock order (**workspace → admission**, taken only by
  `worker::run`, `worker::failed` and `teardown::finish_after_shutdown`).

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
- Structural findings from the Clean Code review are **not** addressed yet. The
  rename pass above made them easier to see, not smaller:
  - `ProjectionCoordinator` is still one type with 13 fields, 5 locks and ~76
    methods spread over nine `impl` blocks. The 350-line file rule was satisfied
    by splitting files, not responsibilities; every one of those files can still
    reach all 13 fields. Candidate collaborators: staging queue, native
    workspace, blocking I/O, source reaper.
  - `BlockingJob` and `IoResult` are parallel enums that must agree by
    convention. `completion::finish_io` carries four defensive "wrong result
    kind" arms that a typed per-job mailbox would delete outright.
  - The enqueue sequence (push, drop lock, wake, fail-on-wake-error) is repeated
    five times across `admission.rs` and `snapshot.rs`.
  - `worker::run` is the real state machine but leaves it implicit: ~100 lines,
    twelve early returns, and it re-reads `status()` three times, so it can
    observe two different residencies within one run.
  - `compatibility` is a bare string whose "non-empty, ≤ 4096 bytes" rule is
    re-derived independently in `projection/coordinator.rs`,
    `checkpoint/protector.rs` and `checkpoint/file.rs`. A domain newtype would
    hold it in one place; today a custom terminal factory can bypass the check.
  - Control generations, park-operation generations and transfer sequences are
    all bare `u64`, and two of them are spelled `generation`.

## Where to look next

| Want | Go here |
| --- | --- |
| How to embed | [`usage.md`](usage.md) |
| Behavior by area | [`features/`](features/) |
| Why we chose this | [`adr/`](adr/) |
| Early measurements | [`experiments/`](experiments/) |
