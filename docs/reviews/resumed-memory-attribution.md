# Resumed memory attribution acceptance review

Reviewed current source over HEAD `46dbec64384e4519e751086ef47feb5bac11024d`
on 2026-09-08. Independent, read-only source assessment of
`candidate6-memory-attribution-plan.md`, ADR 0002, fixture allocation/report/
population/run code, process reader/registration/supervisor ownership, and physical
census collectors. No load, build, gate, or new measurements were run. Prior trial
figures in the plan were not independently recomputed here.

**The plan's two pending acceptance gaps remain valid. Current source cannot
establish a useful baseline-release bound or an atomic settled heap/scratch
snapshot for the existing records. No physical reader-stack attribution can be
recovered from their aggregate process metrics. These are acceptance evidence
blockers, not findings that production exceeds its memory budget.**

## What ownership does establish

`Population::new` reserves the fixture child vector before `runtime`
(`examples/release_load_support/population.rs:45-52`). Its capacity is retained
through ready/measurement and is proportional to configured population. The
runtime itself and diagnostics also remain owned. This supports separately
reporting capacity-dependent shared baseline; it does not establish that every
allocation counted at the baseline remains live.

`ReaderScratch` allocates once, records actual capacity, and never resizes during
`read_loop` (`crates/infrastructure/src/process/io.rs:12-38`). Its allocation is
released at reader exit, before its gauge guard drops. Thus live healthy readers
have stable scratch ownership. A normal idle execution supports the expected
subtraction, but two observations of the same count do not prove no exit occurred
between the gauge and allocator reads; an independently established live-reader
boundary is needed for a strict bound.

## Why existing records cannot close the strict bound

- Baseline memory is not all pinned. `report::checkpoint` allocates strings before
  sampling and drops them on return (`report.rs:4-19`). The supervisor constructs
  and later drops/replaces its poll descriptor vector each loop
  (`process/supervisor.rs:56-83`). Worker startup and library allocations are also
  not classified by allocation identity. Persisting the top-level owners does
  not prove persistence of their entire transitive allocation history.
- The allocator records net live bytes, peak, and allocation count, not released
  bytes or baseline allocation identity (`allocator.rs:34-54`). Counts do not
  bound bytes released. A universally conservative baseline-release bound is
  `F <= H_runtime`; using it eliminates the baseline subtraction and is too loose
  to establish the target from the cited records. Some retained allocations can
  be subtracted from this bound by a separate complete ownership inventory, but
  this review found no complete inventory or useful proven remainder.
- Gauge reads precede formatting allocations and a separate relaxed allocator
  read (`report.rs:4-11`). In addition, realloc subtracts the old size before
  adding the replacement (`allocator.rs:41-44`), allowing a concurrent snapshot
  to see an intermediate value. Reset functions named `quiescent` perform stores;
  they do not rendezvous with workers. The stdin census handshake pauses the
  reporting thread only. Switching to stronger atomic ordering alone would not
  provide a multi-counter snapshot or worker quiescence.

## Minimal useful portable measurement change

Use a dedicated opt-in memory qualification mode rather than expanding the
public domain model. Keep OS stack reporting in infrastructure, and allocation
accounting in the fixture. Freeze source/binary/toolchain identity for its runs.

1. Add allocation **baseline cohort** accounting to the fixture allocator: at a
   consistent baseline boundary, identify the allocations currently live; track
   exactly how many of those original requested bytes are subsequently released.
   A reallocated baseline block releases its old cohort size and makes the new
   allocation post-baseline, including in-place realloc. Allocation failure must
   leave the old cohort unchanged. This needs allocation identity, not a thread
   local tag, because allocation and destruction can occur on different workers.
   A bounded nonallocating pointer ledger or carefully aligned allocator metadata
   can implement it; overflow must invalidate the sample. Record instrumentation
   storage separately and retain requested-byte semantics. No per-session
   allocation classes are required for this conservative first approach.
2. Make sampling detect overlapping allocator/gauge transitions: use an explicit
   activity generation plus in-flight mutation protocol covering alloc/dealloc/
   realloc and scratch registration/release. Accept a snapshot only when no
   relevant mutation overlaps it; otherwise retry with a deadline and report
   unavailable. Serialize the accepted scalar record afterward. A proper
   allocation-free worker rendezvous is an alternative, but blocking workers
   inside allocator locks would be inappropriate. Stable counter values alone
   are not sufficient because intermediate allocate/free activity can cancel out.
3. Calculate `U = H_idle - H_runtime + F_baseline_released - S_idle + S_runtime`
   at consistent successful idle snapshots. This is a conservative upper bound
   on newly live requested heap other than scratch: keep positive fixture and
   shared growth charged, and require raw/no retained payload/no projection.
   Report shared baseline and its capacity separately. Acceptance requires
   `U / N <= 4096` for the required cases, plus a clear assignment of any
   population-dependent baseline reservations; do not hide per-session costs in
   an unexplained shared category. If the conservative bound fails, report that
   it is inconclusive before attempting finer allocation classification.

A cumulative freed-byte counter would be smaller to implement and could bound
`F`, but includes all temporary post-baseline spawn/report allocations. It may be
useful as an initial conservative experiment; it is unlikely to provide the tight
ownership proof sought here. Neither technique retroactively fixes old records.

## Stack attribution requires platform-specific observation

`process/registration.rs:44-47` requests `reader_stack_bytes`; this is not the OS
mapping extent. The reader currently publishes neither native thread identity
nor stack extent. Linux `smaps_rollup` and Darwin `ps` RSS in
`scripts/guardian/resources.py` aggregate all mappings, while
`scripts/release/load_support/census.py` adds thread counts only. There is no
mapping-level information to recover afterward.

The smallest shared contract is a qualification-only record per owned reader:
reader/session generation, native thread identity, usable stack address extent,
guard extent or explicit unavailable status, requested bytes, and a lifetime
lease that prevents reaping/reuse while the collector samples. Query the actual
stack on that reader through platform thread facilities, before entering its idle
read loop. This preserves `thread::Builder` ownership and measures the real
production stack; replacing it with manually allocated stacks would change the
thing being qualified.

Use separate Linux and Darwin adapters to correlate those address extents with
OS VM accounting. Report usable reserved virtual extent, guards, resident pages,
and available private/dirty/committed measures with their actual platform
definitions. If a mapping crosses the stack boundary, do not prorate its aggregate
RSS: use page-level attribution or mark the field unavailable. Committed and
resident are not synonyms, and neither is inferred from requested size. Capture
mapping and reader generations before/after collection and reject a changed
population. Avoid reading/touching stack pages merely to count them, which would
alter residency. Distinguish reader, main/shared-worker, and fixture-child stacks.

This stack census and the heap ownership mode can run against the same stable
idle population, but neither proves stack safety, peak stack use, or the broader
release repetition/soak requirements. Their overhead and measurement scope must
be reported independently from throughput/CPU qualification.
