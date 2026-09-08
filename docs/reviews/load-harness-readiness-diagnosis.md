# Full attached-load harness failure diagnosis

The failed frozen run is not an accepted five-trial result. Trials 1, 3 and 4
failed; trials 2 and 5 completed. The original JSONL evidence is untouched under
`work/coverage-matrix-1/docs/verification/release/load-attached-full-1` and its
hashes are recorded in `docs/verification/release/load-harness-diagnosis/source.json`.

## Exact Python failure site

Trial 1 records `AssertionError('')` after periodic resource sampling. In the
frozen `scripts/guardian/resources.py:35`, that is the assertion on lsof exit
status with empty stderr: `assert inventory.returncode == 0, inventory.stderr`.
The other bare row-count assertion would render `AssertionError()` instead.
`load_support/census.py` catches the initial collection failure and retries
owner-only sampling; the escaping failure therefore comes from owner-only lsof.
The retained record lacks its exit code and stdout.

This is collector failure evidence, not a Rust budget or accounting assertion.
The old driver discarded queued process output and exit status when census
failed, so it is impossible to establish from trial 1 whether lsof failed after
an early owner exit. It must remain failed; no lost event is inferred as a pass.

## Independently reproduced fixture handshake defect

Trials 3 and 4 retained the executable's final error:
`Error: Os { code: 35, kind: WouldBlock, message: "Resource temporarily unavailable" }`,
then exit 1 with no complete event. The load fixture accepts a UnixStream from a
nonblocking listener, sets socket timeouts, and immediately reads its readiness
frame. On macOS the accepted stream inherits nonblocking mode; read timeouts
do not clear it. A child that has connected but has not yet written readiness
can therefore make that immediate read fail. Under-load cancellation probes use
this same spawn path repeatedly.

A standalone std-only listener/stream reproduction returned WouldBlock in 0 ms,
then read the same delayed frame successfully after explicitly switching to
blocking mode (103 ms). This demonstrates a fixture defect independently of the
PTY runtime, native terminal engine or global host limits. It matches the errors
in trials 3/4. Those old errors did not include a Rust call-site backtrace, so this
is not a claim that every old failure has been uniquely attributed.

The canonical fixture now performs its bounded readiness handshake in explicit
blocking mode and restores nonblocking mode for subsequent control polling.
No timeout was increased, and no runtime, native, quota or oracle code changed.
The frozen tree was not edited. A fresh built harness/run is required; the prior
performance results are not retroactively accepted.

## Diagnostics and verification

The canonical resource collector now names lsof/ps, requested/missing PIDs, exit
status and command output. The trial driver records Python traceback and the last
event/checkpoint, preserves queued child stderr/events after failure, and reports
whether the process had already exited or was killed by driver cleanup. Its
failure decision remains unchanged.

Evidence copied under `docs/verification/release/load-harness-diagnosis`:

- `load-accept-race.rs` and `.log`: actual macOS inherited-mode reproduction.
- `load-handshake-before.log`: the focused readiness regression fails with
  WouldBlock before correction.
- `load-handshake-after.log`: the same regression passes after correction and
  confirms subsequent control reads are nonblocking.
- `load-diagnostics-before.log`: three diagnostic regressions fail before changes.
- `load-diagnostics-after2.log`: all three pass, including a real short-lived
  fixture process whose exit 17 and queued stderr survive census failure.

Commands: `cargo test --locked --example release_load
readiness_waits_for_delayed_frame_on_inherited_nonblocking_socket`,
`python3 -m unittest discover -s scripts/tests -p test_load_diagnostics.py -v`,
and `rustfmt --edition 2024 --check examples/release_load_support/population.rs`.
The first two have retained red/green records; formatting passes. Full throughput
repetition is root-owned and was not duplicated during its active trials.


## Independent follow-up findings

The independent reviewer found two harness P2s after the initial diagnosis.
Failure-path buffered stdin close could rethrow BrokenPipe after an unsuccessful
checkpoint acknowledgement; it now records cleanup OSError and returns the failed
trial result, allowing later repetitions and summary publication. A successful
lsof result could also omit a transient process still present in the earlier ps
snapshot; that previously raised uncaught KeyError. Missing lsof identities now
raise descriptive ProcessLookupError and use the existing owner-only fallback,
retaining missing-PID evidence instead of killing a healthy owner.

Both have failing-before and passing-after fixtures in this evidence directory:
`load-cleanup-before.log` / `load-cleanup-after.log`, and
`load-vanished-before.log` / `load-vanished-after.log`. All five diagnostic tests
now pass. Independent re-review passed with all five tests re-executed; see
`docs/reviews/load-harness-independent.md`. The full load run remains pending.
