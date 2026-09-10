# Deterministic tests for three previously uncovered behavioral contracts

Implements the proposals in `docs/archive/reviews/coverage-behavioral-contract-audit.md` as six tests across three contract areas. No production behavior was changed by this task; only test bodies and cfg(test) module registration were added. Other agents concurrently changed parser-control capacity behavior; those changes are outside this task's authorship and verification claims. The macOS host had full load work active; these unit timings are not performance evidence. No Linux/native/crash work was run.

- Projection admission rollback: a rejecting test-only factory holds creation until a lookup handle and completion waiter exist. Failure wakes the pending waiter and preserves exact admission_error, exit=None, supervision_error=None and Eof drain. Backend invocation stays zero, logical reservations return to baseline, and a second attempt with the same ID/single session slot reaches factory creation with a fresh lifetime.
- Completion facts: pure domain tests preserve first exit/drain under equal and contradictory repeats, preserve first admission/supervision failure, require drain for completion and reject cancellation after completion. Application callback tests independently convert contradictions into supervision_error without replacing actual facts; supervision callback counters increment per report and no exit is invented.
- Replay reclamation: storage capacity becomes zero while absolute end/lifetime remain, old cursors see the exact gap on repeated reads, empty release is idempotent, and a subsequent append continues at the old end with exact bytes.

`initial-test.log` preserves the first combined run. It contains two independent `control_capacity` failures being handled by the coordinating task and a **test fixture setup error**, not a discovered production defect: the borrowed tiny feed limit was 16 while default process read_chunk was larger, so normal configuration validation rejected before factory entry, causing the test's entry timeout. The fixture now explicitly sets read_chunk=feed_bytes. No failing runtime behavior was established by these new tests, so there is no claimed behavioral red-to-green production fix.

Commands use the isolated `CARGO_TARGET_DIR=work/coverage-contract-test-target`:

- `cargo test --locked -p pty-runtime-domain`: 27 unit tests + 1 seeded integration test passed (`domain-tests.log`).
- `cargo test --locked -p pty-runtime-application runtime::context_tests`: 3 passed (`context-tests.log`).
- `cargo test --locked -p pty-runtime-application admission_rollback`: 1 passed, including the final pending-waker assertion (`admission-test.log`).
- `cargo clippy --locked -p pty-runtime-domain -p pty-runtime-application --all-targets -- -D warnings`: passed (`clippy.log`).

Selected final source hashes are in `source.json`; root owns the repository gate and independent re-review. No fresh coverage measurement was performed and no coverage percentage improvement is claimed. The historical coverage target remains unmet until remeasured with its full scope.

Root independently inspected the test substance and accepted it, with a factual wording correction: `max_sessions=1` supplies one session slot; the global native reservation limit remains at its default. The assertions directly checking zero native reservations establish reservation release. This is **root static review**, not independent test execution. The author-run test logs above remain labeled as such. Root corrected the corresponding source comment after the first gate passed; the test-source manifest was refreshed afterward. No behavioral test change was made.
