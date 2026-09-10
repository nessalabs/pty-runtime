# First complete Rust coverage baseline

**The 100% readiness target is not met.** The recorded macOS arm64 Rust workspace
run passed its tests, then exited 1 because the coverage thresholds were not met.
This is an initial frozen-source measurement, not final-source qualification.

| Metric | Covered / measured | Coverage |
| --- | --- | --- |
| Lines | 7,254 / 8,006 | 90.61% |
| Functions | 704 / 839 | 83.91% |
| Regions | 10,417 / 11,813 | 88.18% |
| Branches / MC/DC | No instrumented counters | Unmeasured |

The exact command, test log, platform, compiler and file hashes are in
[metadata.json](metadata.json) and [command.log](command.log). The complete LLVM
export is [coverage.json.gz](coverage.json.gz); [summary.json](summary.json)
retains per-file counters and the uncompressed export hash. This export includes
workspace source and test code as reported by cargo-llvm-cov, without a custom
production exclusion filter. It does not measure the entire repository.

The run used all targets and all features. Separate raw/event feature-matrix
collection, helper execution, native C, build tooling, other platform branches,
and final-source remeasurement remain required. The helper was intentionally
uninstrumented for the exact workload environment contract; its binary hash and
override are in [instrumentation.json](instrumentation.json). The
[independent diagnosis](../../archive/reviews/coverage-environment-fixture.md) documents
why helper coverage must use a separate run. The strict environment assertion
continues to reject unexpected exec-boundary variables.

The largest measured gaps were projection teardown, native-work orchestration,
storage I/O failures, process admission and terminal boundary conversion. Later
independent acceptance tests, shutdown fallback tests, redaction tests, and
native corrections are not retroactively credited to this baseline.

The additional readiness command is:

```sh
python3 scripts/coverage.py --output work/coverage/<unique-run-name>
```

It uses fresh run-owned build/profile directories, records each matrix phase,
and requires 100% plus zero uncovered lines/functions/regions. Its Rust/helper
result cannot by itself establish whole-project readiness. See the separate
[own-C coverage record](../native-coverage/README.md), the
[independent test gap audit](../../archive/reviews/independent-acceptance-tests.md), and
[A-10](../requirements.md) for remaining requirements. Code coverage cannot
substitute for behavioral assertions or the full ADR workload counts and soak.
