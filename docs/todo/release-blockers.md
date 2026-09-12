# Release blockers

None of these are small, and none can be closed by a short green run.

## 1. Release load goals

**ADR:** [0002](../adr/0002-performance-and-stability.md) ·
**Feature doc:** [performance-load](../features/performance-load.md)

Large session counts, strict latency budgets and the 12-hour soak are not
closed. Entry points exist (`examples/release_load.rs`,
`examples/release_stress.rs`, `scripts/release/`), and short runs pass — that is
not the requirement. The ADR asks for recorded workload, source identity,
platform and raw results, with repetitions.

**Needs a decision first:** which hardware, and whether the 12-hour soak runs on
every release or on a cadence. Both change what is worth automating.

**Done when:** the ADR's workloads have been run at the stated scale with results
recorded per its table, and `verification.md` cites them.

## 2. OS / CPU qualification

No claimed target has a fresh, complete qualification pass against current
source. CI covers macOS 15 and Ubuntu 24.04 for the gate; that is not the same
as a qualification pass.

**Done when:** each claimed target has a dated pass on a known revision, and any
target without one is either qualified or removed from the claim.

## 3. Coverage

**Command:** `python3 scripts/coverage.py --output work/coverage/<run-name>`

No coverage run exists against current source. `coding_standards.md` states
100% line, function and region coverage as a readiness gate, with branch
coverage where the toolchain supports it, across the Rust workspace feature
matrix and the separately instrumented guardian helper.

**This target currently contradicts the code**, and that needs resolving before
anyone chases a number:

- `collect`'s empty-mailbox fallback in `projection/state.rs` is unreachable by
  construction — `is_ready()` is checked before `take()`, and a panicking job
  publishes `Err(Worker)` rather than leaving the slot empty.
- `commit_park`'s `engine_idle` argument is unreachable-false; an assertion at
  the call site never fired across the whole suite. It is deliberate defence
  against a future path that commits without draining in-flight native work.

Both are defensive code that cannot be exercised without inventing states the
worker cannot produce. Either the standard admits documented unreachable
defences, or the defences go. `coding_standards.md` says not to change behaviour
solely to reach a percentage, which points at the former.

**Done when:** a run exists against a known revision, the measured scope is
stated, and unreachable-by-design regions are either exempted in writing or
removed.

## 4. Memory packing after park/restore

Reclaiming unused native pages after park/restore is not a finished production
story. Relevant: `docs/features/coverage-native.md`, the native lifecycle gate
in `scripts/`, and the `resident` quota — which currently charges a session's
configured `native_bytes` whether or not the pages are resident.

**Done when:** the ADR's memory table can be filled from real measurements
rather than reservations.
