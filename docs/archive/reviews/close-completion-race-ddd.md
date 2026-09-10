# Close completion race: independent DDD review

Reviewed 2026-09-08 against the selected working-tree hashes below. Applied the repository AGENTS.md, coding standards and ADR 0005. Scope: the close wake-result change, independent close_race tests, and release fixture failure diagnostics. No implementation edits, builds, tests or workloads were performed by this reviewer.

## Result

No P1/P2 DDD, dependency-direction, ownership or error-truthfulness blocker found. The change preserves the authoritative durable cleanup outcome when closure has already finished, while returning the scheduler failure for an unfinished close. This is a scoped source review, not a claim that the full gate, native soak or release qualification passed.

## Findings and evidence

- `projection/admission.rs:183-217` retains application ownership of orchestration. The existing scheduler remains behind `IWorkHandle`; no executor, platform type or storage implementation enters application code. The fix consults the existing `close_outcome` contract after a failed wake instead of treating the continued existence of a scheduler handle as authoritative process state.
- `projection/teardown.rs:9-13` returns Some only under the core lock when policy residency is Closed. Thus a wake failure while still Closing is propagated. An intervening completion can legitimately win before this observation. A later completion after an observed None does not retroactively invalidate the scheduler failure reported at that instant.
- `projection/worker.rs:197-217` persists cleanup failure and Closed before completing the registered waiters and releasing services/handle. The caller's previously admitted ticket remains the completion channel. The fix does not synthesize Ok cleanup or copy a possibly incomplete ticket; it returns the existing wait. Even if the caller sees durable closure just before ticket completion, the worker still owns and completes that ticket.
- Storage failure remains a typed `ProjectionError::Storage` cleanup outcome. The durable check distinguishes Some(Err) from None, so finished failed cleanup returns a completion operation carrying the actual failure instead of replacing it with an incidental Worker error. Unreclaimed storage stays charged through the existing cleanup ledger. Ticket admission failure remains Capacity through the unchanged pair result; the patch creates no new wait allocation or unbounded ownership path.
- The new tests drive the real application cleanup worker at a deterministic capacity notification between Closing publication and wake. The success case checks completed outcome plus released budgets/services/handle/native owner. The storage case verifies Unavailable, three delete attempts, one unreclaimed source and retained storage charge. The unfinished case separately injects both absent and rejecting handles and requires Worker plus Closing/None before a later pump. These are meaningful application orchestration tests; their fake scheduler/storage do not establish real infrastructure or native behavior.
- Inspected the retained Linux RED log in `docs/verification/close-completion-race/linux-red/command.log`: both completed-close cases fail with the old wake behavior, while the unfinished-close case passes. This review did not execute or infer a post-fix GREEN result; the coordinator must retain that evidence and the required gate separately.
- `examples/release_support/soak.rs:5-47` adds executable-boundary diagnostics using public lifetime/session/projection observations and returns the original Result. Error conversion in observe_projection retains the preexisting RuntimeError conversion and timeout behavior. `examples/release_support/mod.rs:97-145` distinguishes cancellation, exit request, wait admission, timeout, completion and forget stages without changing which errors propagate. The ordinary finish entry passes None, preserving its uninstrumented failure path. These fixture diagnostics do not add runtime state, claim recovery, suppress errors, or mark a failed native workload successful. Synthetic seed/lifetime/status logging contains no command, payload or secret bytes at the inspected call sites.

## Remaining verification

Post-fix tests and the full mandatory gate remain coordinator responsibilities. No macOS build or workload was started during the concurrent performance run. Existing native crash/soak and full release readiness gaps remain outside this bounded source review.

## Selected source SHA-256

| Source | SHA-256 |
| --- | --- |
| `crates/application/src/projection/admission.rs` | `c13ae04a2d47d6524bd52ac613ddebdf4ee81800e0c61cdc8969fdb7fd674c8a` |
| `crates/application/src/projection/tests/close_race.rs` | `882028d05d3d036fac46ea05bb22b3e3205a300e8419a60f6c0b5dd96cc023ad` |
| `crates/application/src/projection/tests/mod.rs` | `c2db84eb26a22bfb77b47878d9a8138bd9c95bb7b663e6121fe49c35d8d7d9a5` |
| `crates/application/src/projection/teardown.rs` | `c27f771fcec8f751e35aaea5de0665f98844a13050a300783264b53056bcac22` |
| `crates/application/src/projection/worker.rs` | `0c196c99d94ad93c7487e23d256894567389215815450eb77e6b7d726c14e5ef` |
| `crates/application/src/projection/coordinator.rs` | `1d4a2f5bf79ae25de2a4f1c03a7c213568156fed5627b97b313cd5b3d57181b0` |
| `crates/application/src/projection/observation.rs` | `9f42977b3af1aed5ba5e8a888b84cf0b85a77127e1e5dd9cba9f6cf2757f15cb` |
| `examples/release_support/soak.rs` | `7faea6420df8dbd1b04e35a53341efe9aa5d924bb0a024260357f257a835d128` |
| `examples/release_support/mod.rs` | `b5ed1f5f5230a7fce4a41df12967898d73ac9ed92a8ca10cd166b559c454a866` |
