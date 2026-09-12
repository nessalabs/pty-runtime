# Code cleanups

Small and self-contained. Each says what is wrong and what done looks like.

## Files that partition a method list rather than a responsibility

**Where:** `crates/application/src/projection/` — `admission.rs`,
`completion.rs`, `io.rs`, `stream_end.rs` and `teardown.rs` define no type of
their own. They are `impl ProjectionCoordinator` blocks split across files.

An earlier review counted eight such files; after the extractions that figure is
stale. `native.rs`, `snapshot.rs`, `worker.rs` and `inflight.rs` now own
`WorkWake`, `SnapshotRequest`, `Phase` and `InFlight` respectively. `blocking.rs`
owns no type either but is deliberate — free functions over injected ports, which
is what made them testable without a coordinator.

`io.rs` is the remaining large case: two `impl ProjectionCoordinator` methods,
`start_read` and `start_park`, and no type. Read [`declined.md`](declined.md)
first — the reason they are still there is recorded, and it is not that nobody
looked.

`stream_end.rs` is the clearest remaining case: 23 nonblank lines holding one
public API method and one worker step, unrelated to each other.

**Note before starting:** splitting files further is not the fix — that is what
produced this. The fix is types that own state, which is what `AdmissionQueue`,
`SourceReaper` and `blocking.rs` did. Read [`declined.md`](declined.md) first:
the largest remaining candidate was measured and rejected.

**Done when:** each file either owns a type or is merged into one that does.
