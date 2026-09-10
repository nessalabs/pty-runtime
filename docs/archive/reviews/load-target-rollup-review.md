# Load target rollup and CPU-time review

The target-summary finding is valid: earlier summaries exposed correctness-only
`all_trials_passed` without the separately recorded target outcomes. Canonical
summaries now label that scope and include per-trial latency/idle-CPU evidence
plus separate aggregate passed, failed, applicable and unmeasured counts. A target
failure stays visible even when every correctness trial completes. Missing target
records cannot turn into a pass, and inapplicable targets remain explicit.
Idle duration qualification is counted separately. The initial implementation
misclassified missing CPU as a failed target; the later P2 correction in
`load-target-tristate.md` records unmeasured/null and preserves actual failures.
The original frozen artifacts and correctness exit semantics are unchanged.

ADR 0002, lines 211–223, defines dispatch/output/control p99 thresholds and
requires a separate fixture round-trip measurement. The review's comparison of
fixture RTT maximum against the dispatch p99 ceiling is not an ADR violation:
the boundary and statistic differ. No new RTT threshold is invented here.
Throughput/capacity qualification and CPU interval limitations are root-owned
work outside this subtask.

The claimed macOS CPU parser failure after one hour is not established. Local
`man ps` defines `time` as accumulated user-plus-system CPU time but does not
specify an hours-delimited format. Actual local `ps -axo time=` observations above
one hour use growing minutes, including `1295:17.02` and `520:12.90`; the existing
minutes/seconds parser handles these. It is Linux that uses /proc numerical
counters in this collector, so a hypothetical Linux ps format is irrelevant.
The parser was left unchanged. Added tests cover 59:59.99, 60:00.00 and 1295:17.02;
no claim is made about unsupported formats on other operating systems.

Evidence is under `docs/verification/release/load-target-review`: local ps manual,
local long-time samples, file identities and the complete load Python test run.
All 14 tests pass, including root's three census tests, new report integration
and missing-CPU tests, and the existing harness failure regressions. An integrated
summary test confirms correctness true alongside failed latency in summary.json.
No runtime/native code, full workload, or frozen candidate was changed or run.
Root owns the next full gate and performance qualification.
