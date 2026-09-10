# Independent ordinary load-report review

Reviewed against f9da4e1 on 2026-09-08. Scope: release/load.py,
load_support/reporting.py and census.py; the load target, census and diagnostic
tests; adjacent LOAD.md claim wording. Product/native code was not changed.
Exact reviewed hashes and the final unchanged-source validation are retained in
`docs/verification/load-report-independent/`.

No new blocking P1/P2 finding. All 14 focused tests pass:

```sh
python3 -m unittest discover -s scripts/tests -p 'test_load_*.py' -v
```

The reporting integration test was added during the initial review; final
validation binds the completed test source, including its main-to-summary path.
This is a focused reporting check, not a full throughput, CPU, platform or
release-acceptance execution.

## Correctness versus target attainment

The existing trial return/exit behavior continues representing successful
execution and correctness/accounting assertions. The summary now labels that
scope explicitly and carries separate per-trial and aggregate target results.
The end-to-end summary regression deliberately combines a successful execution
with a failed InputDispatch target and proves both results survive serialization.
A zero driver exit or all_trials_passed=true alone is therefore not a performance
acceptance result; consumers must inspect target_rollup and duration/matrix fields.

Expected latency boundaries depend on active/raw configuration. Missing required
boundary events remain named, cannot produce true, and contribute an unmeasured
trial unless another recorded failure already establishes false. A raw trial
correctly omits the projected-output boundary. Idle cases have no active latency
targets, and their CPU target applicability and acceptance duration are separate.
No applicable trials yield null rather than vacuous success. Smoke results and
incomplete trial/matrix counts remain explicitly insufficient for acceptance.

## CPU and census disclosure

The census now returns null CPU totals/percentages when a category has no matched
PID, instead of a misleading measured zero. Before-only, after-only and unavailable
PID lists survive into the interval. complete_process_tree_accounting is always
false, correctly acknowledging processes that start and finish between snapshots
as well as visible unmatched processes. The measurement is explicitly a delta
for numeric PIDs present in both samples, not a complete descendant-lifetime
CPU ledger or a proof against PID reuse between samples.

An unavailable owner percentage cannot pass the idle <=1% target. The recorded
CPU field remains null while the target's passed field is false, so this is an
unmet evidence target, not measured excessive CPU and not a runtime-correctness
failure. Consumers seeking that distinction must retain the raw target/interval
fields. Darwin CPU parsing regression checks minutes beyond an hour; it does not
pretend to have executed a multi-hour load run.

## Non-blocking observation

The successful trial path still leaves Popen stdin/stdout to object finalization
instead of explicitly closing them. The new successful fixture test emits two
ResourceWarnings for unclosed streams; those warnings are retained in the raw
log. This predates the reporting changes and did not prevent the 14 tests from
passing. A small successful-path stream cleanup would make ownership explicit
and avoid the warnings; no accumulating descriptor leak was demonstrated here.
The already explicit failure-path cleanup behavior remains covered.

## Artifact limits

LOAD.md preserves the distinction among target results, correctness, smoke,
configured duration, unavailable CPU/PSS/wakeups, requested Rust allocation totals
and physical/native/fixture costs. This review does not retroactively change any
previous failed trial, qualify five full repeats, establish a 4KiB control-memory
claim or prove complete process-tree CPU. The root owns final frozen gate and
full ordinary workload execution. No native/crash investigation was performed.


## Successful-pipe cleanup follow-up

The author subsequently centralized OSError-contained stream closure and invokes
it after EOF queue drain, process.wait and a bounded reader join on success as
well as failure. Independent reread confirms stdout is never closed while its
reader is still alive, final queued output is retained before trial_result, and
the existing failed-checkpoint/BrokenPipe containment is preserved. The real
success fixture explicitly checks both owned pipes are closed and final stderr
survives. All 15 focused tests independently pass with
`python3 -W error::ResourceWarning -m unittest discover -s scripts/tests -p
'test_load_*.py' -v`. The earlier non-blocking observation is resolved. Final
follow-up source hashes and log are retained beside the original review evidence.
