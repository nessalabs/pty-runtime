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

So the 0.1 ms `chunk-64` figure now carries five-repeat full-duration weight,
not one trial. `chunk-1` is the worst surviving case at 5.5-6.0 ms; still inside
target, but it is the one to watch.

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

**Still to do:** `experiments/0005-g1-concurrent-pressure.md` lives on
`g1-load-evidence` and has not been updated with any of the above. The
after-numbers, the `128-active` host-limit finding, and the corrected failure
signature all belong in it. Recording the run's `ulimit -n` in the harness
identity block would stop this recurring.

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
