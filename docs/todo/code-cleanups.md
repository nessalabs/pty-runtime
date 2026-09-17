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

**The path instrumentation has since fired, and named the call.** It was
recorded here as having produced no evidence. That is no longer true. On a box
resumed from a snapshot it reported:

    Error: Custom { kind: Other, error: "spawn current_dir s64: No such file or
    directory (os error 2)" }

`std::env::current_dir()` returning `ENOENT` — the fixture's working directory
becoming invalid while it ran. A Python process in the same shell died with
`FileNotFoundError` during `import` at the same moment, which is the identical
cause seen from a different process, so it is the environment rather than the
fixture.

**That is the best candidate yet for this item's original `Os { code: 2, kind:
NotFound }`**, and it fits better than any of the four hypotheses ruled out
above: it is an unlabelled `NotFound` out of `main`, it needs no path race, and
it explains why re-running on a settled host never reproduced it. It is not
proof — the original run's environment cannot be re-examined — but the label
that makes such a report readable is the thing this instrumentation was added
for, and it did its job the first time the condition recurred.

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

**A pid is not a process, and the first fix only noticed half of that.** The
Linux collector makes four separate `/proc` reads per process after a `ps`
snapshot, and the kernel is free to recycle the number between any two of them.
When the replacement belongs to somebody else the read is refused and the
`PermissionError` handler catches it; when it belongs to *us* nothing is refused
and the census reports the newcomer's memory, descriptors and threads as the
runtime's — or splices the predecessor's CPU onto the successor's memory. The
collector now reads `(ppid, starttime)` before and after everything else and
marks the pid unavailable if they differ, and carries `start_ticks` so a later
census can tell the same process from the same number. `cpu_delta` and the
archiver's resource cohort are both keyed on the incarnation rather than on the
number. Darwin cannot name an incarnation from this collector; its exposure is
one `ps` and one `lsof` rather than four reads, and it is recorded as null
rather than guessed.

**Verified on macOS itself**, against real `ps` and `lsof` rather than mocks —
with a child exiting between calls, the survivor is still measured and the dead
pid is named, where the previous code raised and discarded both:

    both alive -> [31630, 31631] vanished []
    one exited -> [31630] vanished [31631]

**`dominant` now has macOS evidence, and it is the fix working under load.**
Five trials on macOS arm64, all passing, `ProjectedOutput` p99 0.20-0.30 ms
against a 20 ms target and the offered 10.0 MiB/s held, teardown to one process
with no zombies. Three to four censuses *per trial* recorded a vanished process:
that is the same race, occurring at the same rate, no longer fatal.

**Still open:** the other 10 lost macOS trials, spread across `rate-40MiB`,
`capacity-projected`, `rate-20MiB`, `capacity-raw` and `stalled-sink`, have not
been re-run.

## The seventh raw-child failure, and two mechanisms it was not

**The failure finally said something**, because of the diagnostic added for the
item below. CI, macOS, `76bd423`:

    truncated delivery, not an environment result: 0 bytes, 0 assignment(s),
    names [], 0 unnamed line(s) withheld, final line terminated: false,
    valid utf-8: true

Child exit 0, `DrainOutcome::Eof`, **zero bytes**. The same commit passed on a
second macOS runner, so this is the historical flake and not the change under
review.

**What it is not, measured rather than reasoned.** Two mechanisms were proposed
here and both are now ruled out, which is recorded because each looked
convincing and one of them is a real kernel behaviour that will tempt the next
reader.

*Not a lost PTY buffer.* macOS really does discard a PTY's buffered output, but
the trigger is **reaping** the child, not the child exiting — 200 rounds each,
same 50 ms delay, isolated C probe:

| | Rounds | Delivered | Lost |
| --- | ---: | ---: | ---: |
| Drain the master, then `waitpid` | 200 | **200** | 0 |
| `waitpid`, then drain the master | 200 | 0 | **200** |

*Not that, through the runtime either.* Delaying the reader's first poll by
0, 2, 20, 100 and 500 ms — a temporary env-gated sleep in `read_loop`, reverted
— the test passes every time. Whatever holds the tty alive in the real spawn
path, a late reader does not lose the bytes. A first probe that reported
200/200 loss did so because it reaped before draining and never modelled the
runtime's ordering at all; it proved a kernel behaviour, not this failure.

**What it most likely is, and why the report pointed the wrong way.**
`env -i /usr/bin/env` prints **zero bytes** and exits 0. So the observed
signature is exactly what a child with an *empty environment* produces: delivery
was complete, and there was nothing to deliver. That is this test's own subject
— `PTY_SYNTHETIC_FLAG=final-override` not reaching the child — rather than a
transport fault.

The diagnostic called it "truncated delivery" because its first claim was
`ends_with('\n')`, and `""` does not end in a newline. **It mislabelled the one
case it most needed to name**, and sent this investigation after a lost-output
theory. The assertion is now split: nothing delivered is reported as an empty
environment, and an unterminated final line is reported as a short read.

**Still open:** why the variable intermittently fails to reach the child. The
next occurrence will say which of the two it was, which is what six failures and
then a seventh could not.

## Raw-child flake: diagnosable

`raw_child_contract::empty_environment_is_exact_at_uninstrumented_exec_boundary`
printed nothing on failure, deliberately, to avoid leaking environment values —
which left a truncated read and a real leak looking identical across six
failures, one of them in CI on a documentation-only pull request.

It now reports variable **names**, byte count, and whether the final line was
terminated, with values withheld. A short count with an unterminated line is a
truncated read; an extra name is a leak. **The seventh failure carried a
report**, which is the first time any of them did — see the section above for
what it said, what it ruled out, and the one thing it got wrong.

**Four attempts to reproduce it failed.**
On the Linux box, at `981a74a` plus this branch:

| Attempt | Result |
| --- | --- |
| 60 runs of the test alone under 32-way CPU contention on 8 cores | 0 failures |
| The four historically affected binaries run concurrently, 15 rounds | 0 of 60 |
| Full `cargo test --locked --workspace`, pinned to 2 cores, 3 rounds | 0 failures |

Two earlier guesses were wrong and are recorded so they are not retried. A
variable being inherited from the parent cannot happen: the second argument to
`with_environment` is `removals`, not an inheritance list, so `PATH` never
reaches the child. And `OutputEvent::Complete` cannot overtake pending replay
bytes: the replay read and the completion check happen under one lock, and the
drain outcome is part of the completion record.

A third attempt produced a false positive worth recording. Running the four
binaries as `cargo test --test process_contract` from the workspace root
"failed" 12 times out of 12 — because that target lives in
`pty-runtime-infrastructure` and was never run at all. Cargo said so in the log.
It looked exactly like a clean reproduction.

**Still open:** the mechanism, and therefore the flakiness itself. The
contention timeouts are neither widened nor explained. What has changed is that
the next failure will name which of the three claims broke rather than printing
nothing, which is what made the original six undiagnosable — so the next
occurrence should be worth more than all six previous ones combined.

## The stalled-sink outcome was reported twice: fixed

Found by the archiver's own new check, on the first run after it was added. The
fixture printed `stalled_sink` at the end of the measurement phase and again
inside `stop()`, so every trial carried two accounts of one measurement and any
reader keyed on the event name silently kept whichever came last — the exact
shape that had just been reported three times over as identity, terminal and
latency-target records.

The values are identical, so no published figure moves; the artifact committed
before the check existed simply kept the second copy. `stop()` now re-asserts
the clause without reporting it.

## A confident slope through a two-valued series: fixed

`accumulation.py` judged a metric on two things — is the slope distinguishable
from zero, and is it large enough to matter — and a 55-minute soak passed both
for descriptors: +2.62 over the window at significance 4.28. There was no leak.
The series alternates between **1,864 and 1,873** descriptors as the fixture's
transient cancellation probe comes and goes, 329 samples at one value and 294 at
the other, with no monotonic climb anywhere. The fit had found the duty cycle
between them drifting.

Significance grows with sample count; fit quality does not. Over 631 samples a
slope of one part in 700 becomes "certain" while a line through the data
explains **2.8 %** of its variance — a clean synthetic leak explains 99.8 %.

The tool now reports `variance_explained`, and a slope that is significant and
material but non-linear is **inconclusive** rather than flat *or* accumulating.
It exits 2, so nothing reads it as a pass. This is the tool's own stated trap —
"a big slope over five noisy samples is not evidence" — in the one form its two
original checks could not see.

**Three earlier runs called the same series flat** at significance below 0.2.
That was as unjustified as calling this one a leak: none of the four runs can
settle descriptor accumulation over 55 minutes, and declining to assert is the
correct answer for all of them.

## A flat cohort certifying a population it no longer covered: fixed

`accumulation.py` drops the whole-population basis when the measured process
count moves, which is correct — those totals are not comparable. What it then
did was let the *cohort* certify the metric on its own. A child accumulating
descriptors until it stops being measurable, with the stable owner alone left in
the cohort, read as `flat` and exited 0.

"The surviving subset is flat" is not evidence that the processes which left the
measurement stopped accumulating. A flat verdict now requires either
whole-population evidence or a cohort that covers the measured population;
without both it is inconclusive, and exits 2. An *accumulating* verdict still
stands on the cohort alone — growth found in a subset is still growth — and the
report carries the scope it was decided at.

## The redaction helper could print fragments of a multiline value: fixed

`redacted_environment` read newline-delimited `env` output and treated any line
whose prefix looked like an identifier as a variable name. An environment value
may contain a newline, so a fragment of one can have exactly that shape:
`LEAKED_SECRET=prefix\nSENSITIVE_VALUE_FRAGMENT=rest` was reported as two names,
the second of which is part of the first's value. A base64 value ending in `=`
did the same. No character filter can separate these, because the output format
does not carry the distinction.

The child now runs `env -0`, so records are NUL-delimited and a name is a name.
A trailing unterminated record — the signature of a truncated read — is reported
by length and never split. Both platforms support `-0` identically.

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

**The first fix left the same hole open one step along.** Scanning for unknown
`--` tokens says nothing about tokens that do not start with `--`, so
`release_load staging-slots 16` still ran at depth 256 while looking like a
depth-16 run, and `--raw false` enabled raw mode and dropped the word denying
it. The parser now walks the argument stream and consumes every token as a
recognised option or as that option's value; anything left over, a name given
twice, or a name given no value is refused. `argv[0]` is dropped by the caller
rather than special-cased inside the parse.

## `ProcessError::Io` carried no meaning: fixed

`EMFILE`, `ENFILE`, `ENOMEM` and `ENOSPC` now classify as `Capacity` — the word
this boundary already has for admission being full. The 128-session failure
would read `Process(Capacity)`. The errno itself stays out of the domain, as
`ProcessError`'s own note and ADR 0005 require.

`EAGAIN` stays `Io` in the general mapper, because it is exhaustion from `fork`
and "not ready" from a non-blocking read and that boundary cannot tell which.
**At a creation site it can**: nothing is being polled there, so the ambiguity
that justified leaving it unclassified does not exist. `creation_error` maps it
to `Capacity` at the four sites that make a process or a thread — the sentinel
spawn, the reader thread, the spawner and the supervisor — and a process or
thread limit now reads as admission being full rather than as unclassified I/O,
which is the same failure the descriptor limit cost a full matrix run to name.
