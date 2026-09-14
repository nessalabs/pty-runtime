# Code cleanups

Small and self-contained. Each says what is wrong and what done looks like.

## Batching sequence failure: resolved, and it was the host

**Branch:** `chunk-batching` · **Run:** 2026-09-13, box `bx_pvnvgsk9`
(Linux x86_64, 8 vCPU, load average 0.16), `/tmp/g1-batching-4case`.

`serve()` feeds up to 32 queued output chunks per worker run instead of one.

**The four-case run now completes**, and the `NotFound` failure did not recur.
Four cases at five repeats, full 60-second trials, instrumented binary,
batching patch applied: **15 of 20 passed**, all five failures confined to
`128-active`. `chunk-64`, `chunk-1` and `attached` passed 5/5.

`ProjectedOutput` p99, microseconds, against a 20 000 us target:

| Case | Trials | p99 | Before batching |
| --- | --- | --- | --- |
| `chunk-64` | 5/5 pass | 100 (every trial) | 225 000-273 000 |
| `chunk-1` | 5/5 pass | 5500-6000 | not measured |
| `attached` | 5/5 pass | 200-300 | not measured |
| `128-active` | 5/5 fail | - | - |

**These two figures were later corrected.** This run was on kernel `6.8.0-136`
while the baseline it is compared against was on `6.8.0-117`.
[Experiment 0006](../experiments/0006-staging-depth-and-the-same-kernel-control.md)
re-ran them on the baseline's own kernel: `chunk-64` is **0.8-0.9 ms**, not
0.1 ms, and `chunk-1` is **7.7-9.1 ms**, not 5.5-6.0. The improvement is
unchanged in kind — still 280-fold and inside target on all five trials — but
the numbers above were optimistic by roughly eight times.

**Named cause of the `128-active` failure: the host descriptor limit.** Every
trial died at the `runtime` checkpoint — inside `Population::new`, while
spawning the 128 sessions, before any measurement — with `Error: Process(Io)`
and exit 1. The box default is `ulimit -n 1024`. Re-running the same case, same
binary, same duration under `ulimit -n 65535` passes, `ProjectedOutput` p99
3500 us. This is host configuration, not a runtime defect, and it is unrelated
to batching: it fails before a single byte is projected.

**Two things the earlier write-up got wrong**, recorded so the record is
straight:

- It reported the failure as `Error: Os { code: 2, kind: NotFound }`. The
  reproducible failure is `Error: Process(Io)`. No path syscall failed in any
  of the 20 trials.
- It reported *every* trial failing. Only `128-active` fails. Three of four
  cases pass 5/5. That one case was almost certainly the whole of the earlier
  observation.

Both of the original suspects were also graded against the code before the run,
and neither holds. Parking cannot become eligible: the fixture sets
`projection.park_after` to an hour (`population.rs`) and `begin_park` requires
`park_delay(now) == Some(Duration::ZERO)` (`crates/domain/src/projection/policy.rs`).
The census race is a Python-side `ProcessLookupError` recorded as
`trial_failure`, a different layer from a Rust error out of `main`.

**The path instrumentation added on this branch did not fire**, because no path
call failed. It is still worth keeping — a negative control confirms the label
reaches `main` — but it produced no evidence here and should not be described
as having diagnosed anything.

**Residual, and the reason this item is not simply deleted:** `ProcessError::Io`
is a payload-free variant, so the errno never reaches the operator. `EMFILE`
was indistinguishable from any other I/O failure, which is why naming this cost
a full matrix run. Same shape as the raw-child redaction item below: the
redaction instinct is right, the diagnosability is not.

**Experiment 0005 is updated** on `g1-load-evidence` (`8389efa`, `f3d0f93`)
with the after-numbers, the `128-active` host-limit finding and the corrected
failure signature.

**`capacity-projected` is explained, not fixed by batching.** Batching roughly
halves it and does not bring it near target — it helps the paced small-chunk
cases and leaves saturation. Experiment 0006 found the cause: the tail is the
per-session staging queue's residence time, about 0.83 ms per slot. At the
shipped default of 256 slots it is 271-289 ms; at 16 it is **18.4-19.4 ms and
passes 5/5**, with throughput unchanged, control round-trip twelve times better,
and replay eviction falling from 1.46 GiB to zero. Whether to change the default
is a product decision that evidence does not make.

**Reported p99 above 102.4 ms is a maximum, not a percentile.** The diagnostics
histogram (`crates/application/src/diagnostics/histogram.rs`) is 1025 buckets of
100 us, and `percentile_upper_us` returns the recorded maximum for any rank in
the overflow bucket. At `capacity-projected` 99.76-99.79% of samples land there,
so p50 = p95 = p99 = max. Conservative by design, but it means those rows are
upper bounds and the tail is unresolved. `chunk-64`, `chunk-1` and `attached`
sit well inside resolution and are unaffected.

**`128-active` now has five-repeat evidence.** Re-run under `ulimit -n 65535`:
passes 5/5, `ProjectedOutput` p99 **8.0-9.1 ms** against 20 ms, holding
10.0 MiB/s, with zero histogram overflow so these are real percentiles. The
earlier 3.5 ms figure came from one trial and is superseded.

**The descriptor limit is now recorded** by both harnesses
(branch `harness-descriptor-limit`, `77a46d3`), and `LOAD.md` states what the
128-session cases need.

**Still to do:** no macOS after-numbers for any of this, and 21 of 26 cases
were not re-run.

## Resource census process-exit race: fixed

`process_costs` returned rows or raised. A sampled process exiting mid-census
raised, and the raise discarded the whole sample — every other process's
measurement lost because one had ended. That cost 15 of 130 macOS trials in
Experiment 0005 and left `dominant` with no macOS evidence at all.

It now returns `(rows, vanished)`. On Linux each `/proc` read is guarded per
pid; on Darwin a non-zero `lsof` exit is no longer fatal by itself, while a
collector that produced no output at all still raises, because that is the
collector failing rather than a process ending. Verified against the real race
on Linux: with a child exiting between calls, the survivor is still measured and
the dead pid is named.

**Still open:** the macOS matrix has not been re-run, so `dominant` still has no
macOS evidence. The fix removes the cause; only a run produces the evidence.

## Raw-child flake: diagnosable

`raw_child_contract::empty_environment_is_exact_at_uninstrumented_exec_boundary`
printed nothing on failure, deliberately, to avoid leaking environment values —
which left a truncated read and a real leak looking identical across six
failures, one of them in CI on a documentation-only pull request.

It now reports variable **names**, byte count, and whether the final line was
terminated, with values withheld. A short count with an unterminated line is a
truncated read; an extra name is a leak.

**Still open:** the contention timeouts are neither widened nor explained, and
the underlying flakiness is unaddressed — this makes the next failure readable,
not less likely.

## Resource measurements were not retained: fixed

Experiment 0005's artifact was assembled by hand and kept only a closing census
and a filtered event list, so the six resource cases could not be audited and
the claim behind them was withdrawn under review. The raw output had
everything; the archiving step lost it.

`scripts/release/archive.py` now defines retention in code, and
`scripts/tests/test_load_archive.py` pins it. On a real 25-trial run that is
976 KiB from 22 MiB of raw output, retaining the accumulation series, the
steady-state and closing per-process rows, budget peaks, and latency
distributions trimmed to their occupied span.

## A misspelled fixture flag ran the default silently: fixed

The driver sent `--staging_slots`; the fixture reads `--staging-slots`, did not
find it, and used its default. A four-point sweep measured one point four times
and produced a flat result that looked like a clean refutation. The fixture now
refuses any argument outside the names it knows, and the driver hyphenates.

## `ProcessError::Io` carried no meaning: fixed

`EMFILE`, `ENFILE`, `ENOMEM` and `ENOSPC` now classify as `Capacity` — the word
this boundary already has for admission being full. The 128-session failure
would read `Process(Capacity)`. The errno itself stays out of the domain, as
`ProcessError`'s own note and ADR 0005 require; `EAGAIN` stays `Io` because it
is exhaustion from `fork` and "not ready" from a non-blocking read and this
boundary cannot tell which.
