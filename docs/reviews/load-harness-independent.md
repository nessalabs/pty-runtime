# Independent load-harness readiness and diagnosis review — 2026-09-08

**Clean scoped re-review: no remaining P1/P2 finding in the reviewed fixes.**
Two P2 findings were independently reproduced, reported to the implementation
agent, fixed, and re-reviewed. This is a harness review, not acceptance of the
previously failed throughput trials or a full runtime/native gate.

Reviewed: `examples/release_load_support/population.rs`, `scripts/release/load.py`,
`scripts/guardian/resources.py`, `scripts/tests/test_load_diagnostics.py`, and the
collector fallback in `scripts/release/load_support/census.py`. Exact final hashes
and independent test output are under
[load-harness-independent](../verification/release/load-harness-independent/source.json).

## P2 — failure cleanup could abort all remaining trials (resolved)

At the first reviewed `load.py:145`, `process.stdin.close()` was newly added outside
an error guard. A real fixture that printed a readiness checkpoint then exited 17
caused the acknowledgement write to fail with BrokenPipe. The failure and child
exit were recorded correctly, but closing the buffered stdin raised BrokenPipe
again, escaping `trial()` instead of returning False. `main()` would consequently
skip remaining repetitions and its final summary.

Independent reproduction used a real short-lived executable and a successful
census result delayed by 100 ms. The observed outcome was an escaped
`BrokenPipeError(32, 'Broken pipe')`, with `trial_failure_process` preserving exit
17 and `killed_by_driver=false`. This differed from the original census-failure
regression because the acknowledgement attempt leaves a failed buffered write.

The author now catches OSError from failure-path stream closure, records
`trial_cleanup_failure`, and returns False. The new actual-process regression
requires the original failure, exit 17, cleanup failure record, and False result.
Independent re-execution passes. Author red/green evidence is retained in
`work/terminal-corpus-review/load-cleanup-before.log` and
`work/terminal-corpus-review/load-cleanup-after.log`.

## P2 — partial successful lsof census bypassed fallback (resolved)

At the first reviewed `resources.py:51`, direct `fds[int(pid)]` lookup raised
KeyError if a PID seen by ps disappeared before lsof, while lsof still returned
success for another requested PID. `census.sample()` catches ProcessLookupError,
not KeyError, so this expected observation race failed the whole trial and killed
the healthy owner instead of recording unavailable process data. This path
predated the patch but remained relevant to the collector being repaired.

Independent deterministic input—ps rows for 123 and 456, successful lsof output
for 123 only—produced `KeyError(456)`. The fix detects missing lsof identities
before constructing rows and raises a descriptive ProcessLookupError. The new
integrated collector regression requires the fallback owner sample, explicit
`unavailable_pids=[456]`, and the retained lsof/PID reason. Independent re-execution
passes. Author red/green evidence is retained in
`work/terminal-corpus-review/load-vanished-before.log` and
`work/terminal-corpus-review/load-vanished-after.log`.

## Readiness and evidence assessment

The readiness fix explicitly clears inherited nonblocking mode before setting
read/write timeouts and reading the fixed frame, then restores nonblocking mode
for later control polling. The existing 30-second socket timeout is unchanged.
These are socket-operation timeouts, not a new absolute 30-second budget shared
with the preceding accept loop. A readiness error exits spawn; the incomplete
socket is not reused for polling. The delayed-frame regression also checks that
subsequent empty control polling returns immediately. Source review and the
author's retained standalone/red-green evidence support the mode fix.

The driver records the original Python traceback and last event/checkpoint before
cleanup, drains queued child output, and separately records exit status, whether
it sent the cleanup kill, and whether the output reader finished. Cleanup errors
now remain separate evidence. The collector retains command/PID context for the
reviewed empty-lsof, partial-lsof and missing-ps failure paths. The five focused
regressions exercise actual process exit/output preservation plus deterministic
collector failure/fallback outcomes.

Independent checks: all five diagnostic tests pass; Rust formatting and Python
compilation pass. [Commands and exit codes](../verification/release/load-harness-independent/commands.json)
and raw logs are preserved. No native bootstrap/build inputs were touched, and
Cargo/native tests were not rerun during the coordinator's active native rebuild.
Full gates and fresh full throughput repetitions remain coordinator-owned; old
failed trials are not retroactively accepted.
