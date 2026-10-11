# Program Status Protocol

The domain module `pty_runtime::terminal::program_status` implements the bounded
record semantics of [OSC 7501 revision 0.4](https://www.superlogical.com/rex/docs/build/program-status).
A program can report working, blocked, idle, done or error with an optional
hierarchical id, application, progress and plain-text message/title.
These are untrusted reports, not authentication results or approval grants.

## Domain stage

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

## Delivery status

Stage 1 supplies reusable domain behavior and tests only. Projected-session
ownership, `Session::program_status()`, reply ordering, lifecycle propagation and
quota-held snapshots are stage 2. Neither stage is a full runtime release claim.
