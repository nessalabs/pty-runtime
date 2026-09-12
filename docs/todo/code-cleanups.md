# Code cleanups

Small and self-contained. Each says what is wrong and what done looks like.

## Move `Wiring`'s test seams into the harness

**Where:** `crates/application/src/projection/wiring.rs` (the `#[cfg(test)] impl`),
used from `tests/close_race.rs`, `tests/io_parking_faults.rs`,
`tests/pressure.rs`, `tests/lock_order.rs`.

`inject_services` and `inject_handle` are mutation hooks on a production type.
They exist because the harness builds `ProjectionServices` before `create` and
offers no way to alter it, so tests reach into a live projection instead.

**The path**, from the Clean Code review:

- add `Harness::with_services(impl FnOnce(&mut ProjectionServices))` applied
  *before* `create`, and widen the existing `Harness::with_protector` from
  `Arc<Protector>` to `Arc<dyn ICheckpointProtector>`;
- make the `Scheduler` double configurable so scheduler loss is injected through
  the already-injectable `IWorkScheduler` port rather than by swapping the
  handle on a built projection;
- then delete the whole `#[cfg(test)] impl Wiring`.

**One caller genuinely needs a live projection**: `lock_order.rs` uses
`inject_services` precisely because it runs a closure *while the services mutex
is held*, which is how it proves the leaf registration spans the lock. Keep a
seam for that, or find another real leaf holder to test through — do not delete
the test.

**Done when:** no production type carries a mutation hook, and the lock-order
leaf test still fails when `leaf`'s registration is scoped wrongly.

## Files that partition a method list rather than a responsibility

**Where:** `crates/application/src/projection/` — `admission.rs`,
`completion.rs`, `io.rs`, `stream_end.rs` and `teardown.rs` define no type of
their own. They are `impl ProjectionCoordinator` blocks split across files.

An earlier review counted eight such files; after the extractions that figure is
stale. `native.rs`, `snapshot.rs` and `worker.rs` now own `WorkWake`,
`SnapshotRequest` and `Phase` respectively. `blocking.rs` owns no type either but
is deliberate — free functions over injected ports, which is what made them
testable without a coordinator.

`stream_end.rs` is the clearest remaining case: 23 nonblank lines holding one
public API method and one worker step, unrelated to each other.

**Note before starting:** splitting files further is not the fix — that is what
produced this. The fix is types that own state, which is what `AdmissionQueue`,
`SourceReaper` and `blocking.rs` did. Read [`declined.md`](declined.md) first:
the largest remaining candidate was measured and rejected.

**Done when:** each file either owns a type or is merged into one that does.
