# Coverage tooling and shutdown tests: independent review

Reviewed on macOS arm64, 2026-09-08. Both tooling findings below are resolved
for the intended exclusively owned frozen-run execution; the full project
coverage result remains pending. Scope: `scripts/coverage.py` and
`crates/application/src/projection/tests/shutdown.rs`. Production code was read
for context but not edited. Reviewed source hashes and retained command logs are
in `docs/verification/coverage-tool-review/`.

## Original findings (resolved by follow-up below)

- **P2 — invalid feature-matrix commands** (`scripts/coverage.py:74-76`). The
  shared arguments contain `--no-report`; the raw and event-stream commands add
  `--no-clean`. Installed cargo-llvm-cov 0.6.16 rejects this combination before
  compiling. Independent reproduction, `cargo llvm-cov --no-report --no-clean
  --workspace`, exits 1 with the error preserved in `invalid-flags-red.log`.
  Remove the redundant `--no-clean` arguments and verify the actual matrix.
- **P1 — preexisting profiles can enter the acceptance result**
  (`scripts/coverage.py:70-86`). `--no-report` implies `--no-clean`, including the
  first workspace collection and the helper collection. There is no initial
  clean or fresh isolated coverage directory. Thus profile data from older
  executions can contribute hits even when those tests are not executed in this
  run. Matching source manifests before/after does not establish profile
  provenance. Use a new run-owned target/profile directory for each independent
  workspace and helper collection, or explicitly clean each before collection
  with exclusive ownership. Then accumulate the three workspace feature runs
  intentionally and bind both reports to their collection directories. Verify
  that a preexisting unrelated profile cannot change this run's report.

The `--no-report` behavior was also checked in the installed tool's `cli.rs`
(lines 1185-1187); it explicitly sets `no_clean = true`. No full coverage run was
started during concurrent source changes. A successful `--help` invocation is
not execution evidence for either finding.

## Shutdown test assessment

Independent `cargo test --locked -p pty-runtime-application --lib
projection::tests::shutdown` passes all five tests (`shutdown-green.log`). No
blocking correctness finding in these tests. They first require exactly one
accepted job, discard it without running it, and call the documented terminal
shutdown path after the simulated executor has stopped. Assertions cover failed
callers, retained uncertain disk reservation/source, released live memory and
services, staged output, observer/request budgets, and idempotent finish.

These tests specify conservative cleanup after lost job completion. They do not
establish concurrent shutdown safety, successful native checkpoint recovery, or
100% behavioral coverage. The initial oversized feed fixture and wrong resource
field were test-harness corrections, not a demonstrated production red/green
fix. The final tests pass existing production behavior; report them as added
behavioral regression coverage.

## Acceptance limits

The script correctly labels its Rust-only metrics and exclusions; a scoped pass
must not be described as whole-project readiness. Zero uncovered regions and
functions are useful additional checks but cannot demonstrate every behavior or
unmeasured branch. A complete readiness report also needs a reviewed inventory
of production sources/configurations against the measured JSON, with explicit
classification of files with no executable code and platform/cfg exclusions.
This inventory is a remaining evidence requirement, not a demonstrated omitted
production behavior in the current baseline. Native C evidence is separately
owned. The root reviewer owns the frozen full run and final acceptance.

## Follow-up verification

Root removed the redundant `--no-clean` flags and added `workspace-clean` and
`helper-clean`, with each collection conditional on its clean succeeding.
Reviewed hashes are recorded in `source-after.json`.

An independent tiny Cargo workspace exercised the exact clean/collection command
shapes, including all-features, raw, event-stream, and a separate helper workspace.
All succeeded (`clean-matrix-green.log`). To test stale profile prevention, the
fixture first executed a function only tested with an optional feature, verified
raw profiles existed, ran `cargo llvm-cov clean --workspace`, verified those raw
profiles were gone, then collected only the default-disabled tests. The old
function's JSON execution count was zero. Thus old hits did not survive the clean.

Both findings are resolved provided the root's planned frozen mirror exclusively
owns its coverage collection and is not pointed at another concurrent run's
shared target directory. No canonical coverage cleanup was executed in this
review. The fixture proves tool command compatibility and profile reset behavior;
it is not the pending full-project coverage run or full readiness acceptance.


A subsequent source review also confirmed run-owned isolation: `tempfile.mkdtemp`
creates the build root, workspace/helper use distinct `CARGO_TARGET_DIR` values,
the uninstrumented helper has its own third build directory, inherited
`CARGO_TARGET_DIR` is removed, and metadata records the run-owned root. This
addresses the earlier concurrency caveat in the script itself. The completed
coverage-matrix-1 run is owned and reported by the coordinator.
