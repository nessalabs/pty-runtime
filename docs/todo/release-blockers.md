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

The support claim has now been narrowed to match the evidence, which closes the
labelling half of this entry but none of the running half:

| Target | Evidence level | Where the evidence lives |
| --- | --- | --- |
| macOS arm64 | Exercised, not qualified | Experiment 0003 (`docs/experiments/data/0003/macos-*.json`), native adapter tests (`scripts/native/README.md`), the coverage run below |
| Linux x86_64 | Exercised, not qualified | Experiment 0003 (`docs/experiments/data/0003/linux-*.json`) |
| macOS x86_64 | **Unqualified**, no executed evidence | Nothing |
| Linux arm64 | **Unqualified**, no executed evidence | Nothing |

The two unqualified targets need hardware or a VM before anything can change;
neither is reachable from the current GitHub-hosted runner selection.

**Done when:** each claimed target has a dated pass on a known revision, and any
target without one is either qualified or removed from the claim. The unqualified
two are currently labelled rather than removed; removing them from the target
list entirely is a product decision nobody has made.

## 3. Coverage

**Command:** `python3 scripts/coverage.py --output work/coverage/<run-name>`

`coding_standards.md` states 100% line, function and region coverage as a
readiness gate, with branch coverage where the toolchain supports it, across the
Rust workspace feature matrix and the separately instrumented guardian helper.

**Measured** on macOS 15 / aarch64, `cargo-llvm-cov 0.6.16`, before and after
the tests added alongside this entry:

| scope | lines | functions | regions |
| --- | --- | --- | --- |
| workspace, before (`d2ed0bd`) | 92.91% (8400/9041) | 91.67% | 90.91% |
| workspace, after | 93.13% (8452/9075) | 91.75% | 91.13% |
| guardian helper | 12.50% (183/1464) | 16.82% | 14.73% |

That is roughly 620 uncovered lines over 68 files, not the two defensive
branches this entry used to name. Both of those are gone — see the note at the
end — and the total barely moved, because they were never the obstacle. The
whole of the improvement is `projection/native.rs`, 31 uncovered lines down to
10, from tests for reply and resize refusal paths.

**Branch coverage reports 0/0**, which is not 100%: this toolchain emits no
branch records at all, so the standard's "where the toolchain supports it"
clause is currently vacuous. Say so rather than reporting a pass.

### Most of the gap is unmeasured, not untested

Two whole categories run in a different process from the instrumented one, and
`_exit` skips the atexit handler that flushes the counters:

- **`process/image_materialize.rs`** — 109 lines, the single largest gap. The
  never-executed functions are exactly `child_write`, `open_exclusive`,
  `unlink_path`, `abandon`, `failure`, `errno` and the child half of the test
  hook: everything after `fork()`. The parent half of the same file records 39 to
  61 hits. Four tests exercise this code and assert its effects — bytes written,
  permissions, partial file unlinked on abort — so it is tested and simply not
  counted.
- **The guardian helper's 12.5%** is the same thing one level up: it is a
  separate binary driven over a protocol by integration tests, and this
  instrumentation run does not follow it.

Neither is fixable by writing tests. Making the fork child flush its counters
means calling `__llvm_profile_write_file()` between `fork` and `_exit`, which is
precisely the allocation-and-locking the child's safety comment forbids.

**Before anyone chases a number, the scope has to be decided:** either these are
measured by a mechanism that follows child processes, or they are excluded from
the denominator in writing, with the tests that do cover them named.

### What is genuinely uncovered

Most of the rest is reachable error handling nobody has written a test for.
The largest, after the cross-process files: `runtime/context.rs` (29),
`process/guardian.rs` (26), `process/io.rs` (25), `projection/journal.rs` (23),
`process/backend.rs` (21), `scheduling/pool.rs` (17), `process/spawn.rs` (15).

**Some of it is not reachable, and needs deciding, not testing.** In the ten
lines left in `projection/native.rs`:

- `apply_command`'s `terminal: None` arm. `Phase::Detached` sends a worker with
  no terminal to `read_back_source`, so no command is ever applied without one.
  Same shape as the `engine_idle` parameter removed below.
- The `ControlGeneration::next()` exhaustion arm. Genuinely reachable in
  principle, but only after 2^32 resizes, and the generation is not settable
  from outside the policy. Either the policy grows a test seam or this stays
  uncoverable.
- The `Closing` arm when a queued view is applied. Reachable only if `close()`
  lands between `run()` reading residency and `apply_command` using it — a real
  race, but one a test can only produce with threads.

Each wants the same treatment the two below got: made reachable, deleted, or
given a seam that lets a test reach it.

**Done when:** a run exists against a known revision, the measured scope is
stated — including what the cross-process code contributes and how it is
accounted for — and the branch-coverage clause is either satisfied or withdrawn.

### On unreachable defences

This entry used to say the target contradicted the code, citing `collect`'s
empty-mailbox fallback and `commit_park`'s `engine_idle` argument. Both are now
gone, on the principle that an unreachable branch is not defence — nothing can
test it, and a coverage gate cannot tell it from dead code:

- `collect` is gone. Asking "is it finished?" and "what did it say?" were two
  steps, so the second had to handle an emptiness the first had ruled out.
  `InFlight::take_finished` now does both at once, and the emptiness is the
  `None` that means "still running" — an answer the worker gets on most runs.
- `commit_park`'s `engine_idle` parameter is gone. It was always `true`. What it
  was standing in for is an ordering property, and
  `the_worker_never_parks_while_native_work_is_in_flight` checks that by running
  the worker with a resize outstanding past the park deadline.

## 4. Memory packing after park/restore

Reclaiming unused native pages after park/restore is not a finished production
story. Relevant: `docs/features/coverage-native.md`, the native lifecycle gate
in `scripts/`, and the `resident` quota — which currently charges a session's
configured `native_bytes` whether or not the pages are resident.

**Done when:** the ADR's memory table can be filled from real measurements
rather than reservations.
