# Code cleanups

Small and self-contained. Each says what is wrong and what done looks like.

## Explain the batching sequence failure before trusting the fix

**Branch:** `chunk-batching` (commit `56c013f`, not pushed) ·
**Evidence:** [`../experiments/0005-g1-concurrent-pressure.md`](../experiments/0005-g1-concurrent-pressure.md)

`serve()` now feeds up to 32 queued output chunks per worker run instead of one.
On an idle Linux host this moved `chunk-64` `ProjectedOutput` p99 from
225-273 ms to **0.1 ms** against a 20 ms target, and `ResizeDispatch` from
189-252 ms to 0.071 ms, on a full 60-second trial at the same revision.

**It is not finished.** Running four cases at five repeats in one invocation
(`--case chunk-64 --case chunk-1 --case 128-active --case attached`) failed
*every* trial with `Error: Os { code: 2, kind: NotFound }` from the fixture,
exit 1. A single full-duration run of `chunk-64` alone passes cleanly with the
same binary, and a smoke run passes. So it is not simply "the patch is broken",
and it is not yet understood.

Do not repeat the mistake that was made while chasing this: a reverted build was
compared against a *smoke* run and the difference read as proof the patch was at
fault. Compare like for like — same case, same duration, same repeat count.

**First suspect**, untested: the resource census racing process exit. The macOS
matrix lost 15 of 130 trials to exactly that (`ProcessLookupError` from `lsof`),
and batching drains the queue faster, so producers finish sooner. `ENOENT` is
what a `/proc/<pid>` read returns after the process is gone. That would make it
a harness defect this change merely exposes — but that is a hypothesis, not a
finding.

**Second thing to check**, also untested: draining to empty changes when parking
becomes eligible. `begin_park` requires an empty queue, and at `chunk-64` the
backlog previously meant the queue was never empty. Whether that now starts
parking mid-run, and what that does, has not been examined.

**Done when:** the sequence failure has a named cause; the affected cases are
re-run at five repeats each so the after-numbers carry the same weight as the
before-numbers in Experiment 0005; and 0005 is updated with the result.

## Fix the resource census process-exit race

**Where:** `scripts/release/load_support/census.py`, and whatever reads
`/proc/<pid>` on Linux.

The macOS census lost 15 of 130 trials in Experiment 0005 —
9 `ProcessLookupError` from `lsof` plus 6 aborts — and left the
dominant-producer case with **no macOS evidence at all**. Linux completed
130 of 130 in that run, but the `ENOENT` above may be the same race.

This is now the limiting factor on G1 evidence rather than the runtime.

**Done when:** a sampled process exiting mid-census is tolerated and recorded,
not fatal, and the macOS matrix completes 130 of 130 including `dominant`.

## Make the raw-child flake diagnosable

Real-child tests failed under CPU contention **six times** in one working
session, across four files: `raw_child_contract`, `event_stream_failures`,
`process_contract` and `event_stream_decode_contract`. All pass in isolation.
One of them failed CI on an unrelated documentation-only PR.

`raw_child_contract::empty_environment_is_exact_at_uninstrumented_exec_boundary`
fails with "unexpected exec environment" and deliberately does not print the
mismatch ("Compare without printing unexpected values if a future environment
leak occurs"). The redaction instinct is right and `coding_standards.md` requires
it, but as written nobody can tell a truncated read from a leaked variable.

**Done when:** the failure prints something actionable and still redacts values
— observed variable *names* and byte length would do — and the contention
timeouts are either widened or explained.
