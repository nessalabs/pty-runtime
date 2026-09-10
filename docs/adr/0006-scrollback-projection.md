# ADR 0006: Scrollback projection

Status: proposed. No implementation exists. This records the decision and its
constraints before code, because the surface is public and the alternative
considered here is the one that looks obvious and is wrong.

Sources it amends: [ADR 0001](0001-pty-runtime.md) section 4 (Libghostty), and
[ADR 0005](0005-domain-boundaries-and-adapters.md) terminal engine boundary.

## Problem

`ITerminal::view` copies the **active** screen. The engine retains history —
that is what the scrollback limits in ADR 0001 bound, and what
`restore_history_step` reconstructs — but nothing in the domain can read it.

A consumer that wants to show scrollback therefore cannot, and the first one to
try (the demo client in `client/`) has to fall back to translating a wheel into
cursor keys and letting the running program scroll itself. That works only on
the alternate screen, and not at all for a shell.

## The obvious approach, and why it is rejected

Ghostty exposes viewport scrolling (`GHOSTTY_SCROLL_VIEWPORT_*`), so the direct
route is to add a "scroll the viewport" operation and keep projecting the
active screen. It is rejected:

- **It mutates shared state to perform a read.** The viewport is terminal
  state. A reader that moves it changes what every other reader sees, and
  changes what the terminal itself does on the next output: writing while the
  viewport is scrolled back has defined but surprising behaviour.
- **It cannot serve two viewers.** Two attached observers scrolled to different
  positions is a normal thing to want and an incoherent thing to express
  through one mutable viewport.
- **It races with output.** Between scrolling and projecting, a chunk can
  arrive. The caller receives a screen from a position it did not ask for, with
  no way to detect that.
- **It is not restorable state.** A viewport position is a viewing preference;
  putting it in the terminal makes it something checkpointing has to reason
  about.

## Decision

Expose scrollback as a **read-only range query addressed by absolute row
index**, leaving the viewport out of the domain entirely.

- Rows are numbered from the oldest row the engine still holds: index zero is
  the oldest retained row, not the oldest row ever produced. A query names a
  start and a count and receives the rows that exist within it.
- The query is a projection: it copies owned domain values and takes no
  reference to engine memory, exactly as `view` does.
- It never moves the viewport, so it is safe with concurrent output, safe with
  several readers, and invisible to checkpointing.
- Every response reports the current total row count and how many of those are
  scrollback, so a caller can position itself and notice when the window has
  moved beneath it.

Viewing position belongs to the consumer, not the runtime. A client that wants
a scrolled view holds an index and asks for that range.

## Consequences the implementation must handle

- **Eviction is normal, not an error.** Scrollback limits discard the oldest
  rows while a reader is looking at them. A request for a range that has partly
  or wholly aged out returns what still exists, together with the index it
  actually started at and the current totals. It must not fail, and it must not
  silently substitute different rows for the ones asked for.
- **The range is bounded.** A query is admitted against the existing
  `view_bytes` budget in the same way `view` is; an unbounded request is
  refused rather than served.
- **Indices are window-relative and do shift under eviction.** This is a
  concession to what the engine can report, not a preference. Stable indices
  would need a monotonic count of rows ever produced; the native API exposes
  only the current total and scrollback size, and at the scrollback cap a row
  arriving and a row being evicted are indistinguishable from those numbers.
  Synthesising a base from them would be a guess presented as an index, which
  is worse than not offering one.

  Every response therefore carries the current total and scrollback row counts,
  which is what a consumer needs to re-anchor. The visible cost is that a
  consumer scrolled far back can have its position shift beneath it while old
  rows are discarded. Restoring the stronger guarantee needs the engine to
  report rows-ever-produced alongside its total; that is an upstream change,
  recorded here so the concession is not mistaken for a design preference.
- **Wrapped lines stay physical rows.** The projection reports what the grid
  holds, matching `view`. Reflowing into logical lines is a consumer concern
  and would make indices unstable.
- **Restoration interacts.** History arrives incrementally after a restore, so
  a query during restoration reports the rows recovered so far and the caller
  can observe the count growing. It must not block, and it must not imply
  completeness that `restoration_progress` denies.

## Rejected alternatives

- **Return the whole scrollback.** Unbounded by construction; the limits exist
  precisely because it can be large.
- **Stream history through the event stream.** That carries raw output bytes
  for replay, a different thing from projected cells, and would force a
  consumer to run an emulator to use it.
- **Reuse `restore_history_step`.** That reconstructs state from a checkpoint;
  it is not a reader, and coupling viewing to restoration would make one
  observable through the other.

## Proof required

To be added to the ledger as its own row rather than folded into G2-06:

- A range spanning live output and history agrees with an uninterrupted
  reference for the same session.
- Eviction under a configured scrollback limit returns the surviving rows with
  no error, and reports totals that let a consumer detect that its position
  moved.
- Concurrent output during a query leaves the active screen, the cursor, and a
  subsequent checkpoint byte-identical to a run without the query, proving the
  read is genuinely free of side effects.
- A query during incremental restoration reports only recovered rows and never
  contradicts `restoration_progress`.
- Budget refusal is typed and allocates nothing.
