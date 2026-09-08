# Three deterministic behavioral priorities from the strict Rust coverage report

Read-only audit of `docs/verification/coverage-matrix-1/workspace.json.gz`, its source inventory, and current domain/application tests. No source edits, builds, test execution, native work, or workloads were performed. The selected production files exactly match the SHA-256 hashes recorded by that coverage run; selected LLVM region-entry records and source hashes are retained in `docs/verification/coverage-contract-audit/selected-regions.json`.

The historical workspace aggregate is 7483/8162 lines, 720/851 functions and 10757/12043 regions; it includes retained test code and is **not a production-only denominator or current coverage claim**. Zero-count region entries below are observed export facts, not branch-coverage percentages. LLVM branch coverage was not collected.

## 1. Projection admission failure must terminate a published context and roll back ownership

Production gap: `crates/application/src/runtime/owner.rs:125–131` has zero execution throughout the projection-create error arm. `crates/domain/src/session/mod.rs:200–202` (`record_admission_failure`) is also wholly unexecuted. This path runs after repository registration but before any child backend launch, so an observer can already hold the context that must be settled even when registration is rolled back.

Existing coverage: `tests/ddd_spawn_failure.rs` deterministically holds backend spawn behind barriers and verifies an earlier lookup handle completes when the backend fails. That exercises the later supervision failure path, **not** earlier projection admission failure. `tests/projection_correctness.rs::impossible_global_parser_budgets_reject_before_child_launch` checks invalid global/session configuration before the context is admitted; it does not prove the registered-context rollback branch.

Proposed focused test: add `tests/projection_admission_rollback.rs`, reusing the existing application port fixture pattern. A test-only terminal factory's `create` method signals entry, waits for an explicit release, then returns a typed `TerminalError` without creating native state. Other projection services use inert/pumped fixtures; backend counts calls. While `create` is held, acquire a lookup handle and poll its wait once to establish Pending, then release the factory error.

Assertions: spawn returns the exact `RuntimeError::Projection(ProjectionError::Terminal(...))`; backend spawn count stays zero; repository lookup becomes MissingSession; the retained handle's wait resolves with admission_error set, exit=None, supervision_error=None and drain=Eof; it cannot hang. Drop the retained handle and verify session/projection admission resources return to their pre-attempt values. A second attempt with the same ID must reach the factory again rather than ExistingSession/Capacity, proving identity and projection reservation rollback. This uses existing ports and synchronization, with no production fault flag or native engine.

## 2. Contradictory completion facts must preserve the first facts and report an independent failure

Production gap: `crates/domain/src/session/mod.rs:181–182,189–190` reject conflicting exit/drain reports but their disagreement closures and error returns have zero counts; entry counts show ordinary first reports execute 148/151 times. The conversion arms `crates/application/src/runtime/context.rs:316–317,325–326` are likewise unexecuted. The separate `Events::supervision_failed` path at `context.rs:336–344` is wholly unexecuted.

Existing coverage: current raw lifecycle/integration tests establish ordinary completion; `runtime/context_tests.rs` only tests quota release after a poisoned context. Domain session currently has no dedicated test module. Searches of current tests found no repeated/conflicting `record_exit`/`record_drain` or direct `supervision_failed` assertions. A callback duplicate is possible without crashing a real process and deserves a contract test rather than a coverage probe.

Proposed tests: a focused `crates/domain/src/session/tests.rs` for the pure state machine, plus cases in `crates/application/src/runtime/context_tests.rs` using its existing `SessionContext` and `Events` construction. Feed first exit Code(7), repeat Code(7), then conflicting Code(0); analogously feed Eof, repeat Eof, then Failed(typed error). Use separate contexts for order variants. Invoke supervision_failed on a fresh context without exit and then deliver drain.

Assertions: equal duplicates are idempotent; conflicting facts return Internal at the domain boundary while the first exit/drain remain unchanged. At the application callback boundary the same conflict records supervision_error=Internal and never fabricates/replaces exit or drain. Before a final drain, a supervision error alone leaves completion absent; after drain the completion contains the failure with exit=None. With diagnostics enabled, the explicit supervision failure increments FailedOperations exactly once per callback. Cancellation after completion returns Closed and cannot mutate the completed facts. No child, native provider, wall-clock delay or panic injection is necessary.

## 3. Releasing replay allocation must preserve absolute position and report exact loss before new output

Production gap: `crates/domain/src/replay.rs:131–133` (`release_storage`) is wholly unexecuted. This API is the actual allocation reclamation path used when a shared replay reservation cannot support allocator-visible capacity; its contract differs from logical eviction.

Existing coverage: `crates/domain/src/replay_tests.rs` covers suffix eviction, foreign/future cursors, zero retention, overflow and chunking. Its pressure test calls `retain_at_most`, which can retain allocation. It never calls `release_storage`, so it does not prove storage reclamation or continuation after reclamation.

Proposed test: extend `crates/domain/src/replay_tests.rs`. Create a lifetime-bound buffer of limit 8, reserve 8, append `abcdef`, and retain the initial and current-end cursors. Call release_storage twice, then append `gh` with the same lifetime.

Assertions: after release, len=0 and allocated_bytes=0; end remains offset 6 and floor equals that end; reading the original cursor produces exactly Gap(0→6), repeated reading is stable, and reading the old end is Pending. After appending, end is 8, the original cursor still reports Gap(0→6), reading the old end yields exactly `gh` and next.offset=8, with the lifetime unchanged. Releasing an already empty buffer is idempotent. This tests a documented ownership/stream continuity guarantee directly, without trying to force allocator failure or altering code to improve coverage.

## Uncovered production versus test-only code

Do not treat every zero-count symbol as a missing production function. The export contains feature/configuration instantiations: a zero-count instantiation of a function whose merged file regions execute does not show that behavior is wholly uncovered. Use merged file segments, as above. Compiler-generated poison-recovery closures and formatting implementations should not outrank observable state/ownership contracts merely to raise function coverage.

Concrete test-only example: `crates/application/src/diagnostics/tests.rs:15–27` contains `UntimedAdapter::process_id`, `write_reserved` and `request_cancel`, unused methods supplied to satisfy the test double's trait. Their uncovered bodies belong to test scaffolding; calling them solely to green the denominator does not add a runtime contract. The file's 37/50 covered lines must not be reported as 13 uncovered production lines.

Also avoid duplicating behavior already added since this historical run: current `crates/application/src/projection/tests/io_faults.rs` is absent from the recorded source inventory and now tests short ciphertext, wrong opened descriptor/capacity, panicking reads, and rejected transfer-read admission with cleanup. Historical zero regions in those paths are stale coverage evidence until remeasurement, not justification for duplicate tests.

These are proposed deterministic tests, not findings that the implementation is incorrect. A future focused test run and refreshed strict coverage export are needed before claiming any readiness improvement. This audit does not address the separately reported helper, native, platform, Python or branch-coverage gaps.
