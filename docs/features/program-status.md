# Program Status Protocol

The domain module `pty_runtime::terminal::program_status` implements the bounded
record semantics of [OSC 7501 revision 0.4](https://www.superlogical.com/rex/docs/build/program-status).
A program can report working, blocked, idle, done or error with an optional
hierarchical id, application, progress and plain-text message/title.
These are untrusted reports, not authentication results or approval grants.

## Record semantics

`ProgramStatusStore` owns a streaming scanner and at most 256 records (a caller
may choose a cap in 64..=256). Reports replace an id's previous fields. Ancestor
application inheritance is resolved on read; clearing a parent clears descendants.
A shell prompt or process exit clears working/blocked records; a full reset
clears everything. No timers or heartbeat refresh are required.

Call `scan` to prepare a bounded plan without changing committed parser state or
records. Query offsets identify where an authoritative terminal should insert the
fixed `PROGRAM_STATUS_REPLY`. Only call `commit` after the terminal accepted the
whole input. Dropped plans change nothing; commits reject stale or foreign plans.
A terminal error after a prefix was accepted does not authorize replaying that
prefix. The caller must fail that projection and preserve its failure evidence.

The parser leaves original bytes untouched. Raw forwarding does not imply a
terminal emulator and must not generate replies. An embedder which owns terminal
emulation can use this same domain type instead of implementing another parser.

Stored text remains exactly decoded. Debug redacts payloads; separate display
helpers escape invisible formatting characters. Display text as plain text,
never as markup. Records cannot be queried by a child process: the only reply is
a fixed capability response, with no report fields copied into it.

## Bounds and measurements

The scanner retains at most one bounded partial sequence. Each scan has a finite
input limit and exposes a conservative pre-allocation reservation. Record and
snapshot reservations include payload and container capacity, excluding allocator
metadata. Callers own admission for copied snapshots and any further clones.
Invalid reports do not change records or their eviction order. A full record set
evicts the least recently updated id only for an accepted new id.

`python3 scripts/program_status_performance.py` records five release-build trials
for ordinary text, oversized sequences, query floods, saturated records and
single-byte chunking under `work/program-status-performance/`. The mechanical
gate runs it. These are domain workloads, not PTY, OS memory or release-scale
qualification. A new parser has no earlier implementation to compare; stage 2
must separately compare the existing integrated output workload before and after
wiring the parser into the hot path.

## Projected sessions

`Session::program_status()` returns `Ok(Some(snapshot))` for projected sessions
and `Ok(None)` for raw sessions. The snapshot exposes `revision()` and borrowed
`records()`. It owns its quota lease until dropped, including after the session
closes. Copying records from it creates caller-owned allocations. A snapshot is
an observation of the latest processed output, not a barrier waiting for all
published bytes. Use projection processed/published positions when a byte-boundary
barrier is needed.

`ProjectionOptions::program_status_records` defaults to 256 and accepts 64..=256.
Store, scanner, scan plans, replies and copied snapshots use the runtime's shared
`ProjectionLimits::view_bytes` budget. Ordinary text has no protocol heap charge.
Temporary quota pressure retries before any native feed; an allocation that can
never fit fails projection explicitly. Snapshot admission can return capacity
without changing session state.

Only the authoritative projected session emits the fixed capability reply.
Replies are inserted at query boundaries before subsequent native terminal
replies; the whole original chunk shares one reply-byte limit. Reports generate
no reply. Observers and raw sessions never reply automatically. Original replay
and transfer bytes are unchanged.

A chunk's protocol mutations commit only after every native feed slice succeeds.
Any native feed failure, including `HistoryIncomplete`, fails projection without
retrying an accepted prefix. Failed original bytes remain available as diagnostic
staging; protocol records do not pretend the chunk was accepted. A failed or
closed projection returns an error from status reads.

The protocol store belongs to the session rather than the native terminal model.
Parking, restoration and status reads preserve records and revision; status reads
do not restore the model. An ordered process-exit marker clears working/blocked
records even while parked or when ordinary admission is full. Final output drain
clears them again, after buffered reports have been processed. Idle, done and
error records remain until cleared or the session closes. End-of-file alone is
not treated as process exit.

## Delivery and validation

Stage 1 introduces the domain parser/store; stage 2 connects session ownership,
ordered replies, lifecycle facts and memory admission. Validation results and
remaining coverage/release gaps are recorded in `../verification.md` and
`../reviews/`. Neither stage claims full runtime release qualification.
