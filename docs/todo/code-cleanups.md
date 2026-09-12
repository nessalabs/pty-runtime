# Code cleanups

Small and self-contained. Each says what is wrong and what done looks like.

## The blocking-I/O orchestration lifecycle

**Where:** `crates/application/src/projection/io.rs` (`start_read`, `start_park`,
`start_delete`, `submit_io`) and `completion.rs` (`finish_io`,
`history_progress`).

Only the provider-facing job bodies and the submission primitive were extracted
to `blocking.rs`. The lifecycle that drives them is still coordinator methods,
and it is where the coupling lives:

| method | collaborators | workspace fields |
| --- | --- | --- |
| `finish_io` | 5 | 10 |
| `start_read` | 3 | 3 |
| `start_park` | 3 | 3 |
| `history_progress` | 1 | 3 |
| `start_delete` | 0 | 2 |
| `submit_io` | 1 | 0 |
| *extracted jobs, for contrast* | **0** | **0** |

`start_delete` is already nearly free of the coordinator — it reads the reaper
and the in-flight slot and nothing else, so it is the obvious first move.
`finish_io` is the opposite end and is closer in shape to the native engine
driving, which was measured and declined; read [`declined.md`](declined.md)
before attacking it, because the same argument may apply.

**Done when:** the lifecycle is either owned by a type that holds the in-flight
slot and its dependencies, or recorded here with a measurement explaining why
not — as was done for the native engine driving.

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
