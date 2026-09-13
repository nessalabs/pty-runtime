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

**Both original suspects have now been graded against the code. Neither holds.**

*Parking (second suspect) is refuted.* The load fixture sets
`projection.park_after = Duration::from_secs(3600)`
(`examples/release_load_support/population.rs`), and `begin_park` refuses unless
`park_delay(now) == Some(Duration::ZERO)`, computed from
`last_activity + park_after` (`crates/domain/src/projection/policy.rs`). No
60-second trial can reach that, drained queue or not. Batching cannot have made
parking eligible mid-run.

*The census race (first suspect) is the wrong layer.* That race surfaces as a
Python `ProcessLookupError`/`FileNotFoundError` recorded as `trial_failure`, and
`load_support/census.py` already catches those on the batch path. The recorded
symptom — `Error: Os { code: 2, kind: NotFound }`, exit 1 — is Rust's
`Termination` printing an `io::Error` returned out of the fixture's `main`. It
is still a real defect (see the next item); it is not this one.

**Not reproduced on macOS in smoke.** Four cases at one repeat, then at five
repeats (20 trials), all passed. So "several cases in one invocation" is not by
itself the trigger. Smoke clamps sessions to 4 and duration to 2 s, so this
rules out the cheap explanation, not scale or duration.

**Current lead.** Every ENOENT-capable call in the owner is a path operation,
and one path is used unlike all the others. `phase.rs` spawns the transient
cancel probe as `population.spawn(config, config.sessions, true)` once per
second for the whole run, always at the same socket name `s<sessions>`; each
pass binds it and then unconditionally `remove_file`s it after the handshake.
It is the only name bound and unlinked repeatedly, and it only runs when
`config.active > 0`. A 2-second smoke trial fires it at most twice; a
60-second trial fires it about sixty times. That matches "full duration
reproduces, smoke does not" without the multi-case part being causal at all —
which would also mean the single-case `chunk-64` pass was luck, not a control.

**Instrumentation is in place** (commit on this branch): every path syscall in
the fixture now names its site and operand, and distinguishes the transient
probe socket from producer sockets. A negative control confirms the label
reaches `main`:

    Error: Custom { kind: Other, error: "child connect to producer socket s7
    /tmp/pty-load-nonexistent/s7: No such file or directory (os error 2)" }

The next full-duration run on the quiet Linux host should therefore report which
call failed instead of requiring a bisect.

*Unrelated fragility found while reading:* the fixture directory is
`/tmp/pty-load-<pid>` with no randomness, and `Drop` is its only cleanup, so a
driver `process.kill()` leaves it behind. That yields `AlreadyExists`, not
`NotFound`, so it is not this bug.

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
