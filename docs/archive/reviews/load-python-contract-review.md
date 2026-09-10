# Independent Python reporting/dependency review

Scope: Python changes in `scripts/release/load.py`, `load_support/census.py`, new `load_support/reporting.py`, `experiments/gate.py`, and associated new tests. The reviewer authored the Rust capacity fixture and explicitly excludes that Rust implementation and matrix changes from this independent verdict. Review is static plus a pure in-memory reporting reproduction; no workloads, native investigations, or production-source changes were performed.

## P2: unavailable measurements are classified as measured failures (resolved after independent re-review)

`load.py:116` records a missing p99 as `passed=False`; `load.py:125` does the same for `core_percent=None`. `reporting.py:20,27,29` propagates those booleans and infers missing evidence only from missing/null passed status. Thus a present target record with no measurement becomes `failed_trials=1, unmeasured_trials=0`, and missing latency is absent from `latency_targets_missing`. This is fail-closed for passing acceptance, but contradicts the documented distinction between measured target failure and missing required evidence. The existing null-idle regression enshrines the misclassification rather than exercising the new summary contract.

Pure reproduction and exact pre-fix Python hashes are retained at `docs/verification/load-python-contract-review/`. Root agreed the finding and assigned the original reporting author to fix it with red/green tests. Required correction: missing measurement should produce null/unmeasured; actual observed target miss or explicit operation failure may establish false. Missing evidence should remain visible even when another measured failure determines the overall verdict.

## Other scoped conclusions

No DDD/dependency-direction findings: orchestration remains in the CLI; OS census stays behind the focused collector; reporting consumes recorded DTOs with no process/native control; none of these imports crosses into Rust domain/application ownership. The collector now labels matching-PID-only deltas, unmatched snapshots and unavailable process census explicitly, and never claims complete transient-process accounting. It cannot establish whole-tree CPU cost and correctly says so.

Pipe closure uses one helper for success and failure, waits for owner exit and reader completion before closing stdout, preserves final merged stderr, and records cleanup errors. The added real child regression checks final output and descriptor ownership.

The experiment validator now selects memory records by kind before reading memory-specific fields. Separate optional packed-pool records require the exact stage inventory and independently prove zero requested/mapped bytes and balanced maps at parked/cleanup. Legacy memory-only metrics remain supported; tests cover mixed actual records, independent memory/pool retention failures, missing packed stage and legacy metric equality. This is a schema-adapter correction, not a claim of new native behavior.

Independent re-review confirms the correction: target evaluation returns null for absent/incomplete observations, false for observed threshold misses or explicit operation failures, and true only for complete passing evidence. The driver preserves that tri-state verdict in `trial_result`; boundary missing lists and `incomplete_trials` remain visible when a separate actual failure dominates. Correctness success remains independent. The author's retained real-child null-CPU red log was checked, and the reviewer independently reran all 18 load tests with ResourceWarnings as errors (pass). The five pure native-lifecycle validator tests also pass; no native workload was run.

The P2 is resolved. No remaining P1/P2 findings in this independently reviewed Python scope. Final Python hashes and independent test outputs are under `docs/verification/load-python-contract-review/`. The root task owns full gate execution and the remaining specialist reviews; this document does not substitute for those or release qualification.
