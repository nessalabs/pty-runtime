# Close completion race: independent correctness review

Reviewed source on 2026-09-08T21:39:45.464511+00:00 against base `71c1d6f42bbd1c547f747fdd7d20ed69e3000c4e`. Scope: the close wake-result change, three independent regression tests, and fixture-only soak diagnostics. No P1/P2 finding in this change. This is source/test review, not a release acceptance or a diagnosis proving the original soak failure had this cause.

## Close outcome and concurrency

`admission.rs:208-214` suppresses a failed wake only when `close_outcome()` observes `Residency::Closed` under the core mutex. It does not synthesize success or clear the durable failure. The return remains the original ticket-admission result, so Capacity is preserved if no observer slot was available. A successful admission returns the original wait, whose completion contains the actual cleanup result, including storage failure. A missing/rejecting scheduler while the outcome is still absent still returns Worker.

`worker.rs:196-227` publishes the final cleanup failure and Closed under the same mutex, takes all waiters, completes them with that failure, and subsequently releases services and handle. Therefore the fix covers both a removed handle and a cloned handle whose wake rejects after worker completion. If Closed is observed before ticket completion, returning the admitted wait is still correct: worker completion remains responsible for resolving it. `teardown.rs:9-12` reads the same durable pair. Closed cannot be reopened by another close; the existing closed branch returns the stored result.

The patch introduces no new lock held during `wake()`, capacity notification, or worker completion. Existing request admission and teardown ownership remain unchanged. This does not establish every neighboring concurrent-close behavior outside the changed branch.

## Independent test strength and retained evidence

The three tests in `tests/close_race.rs` drive the actual coordinator worker and queued provider work synchronously at the capacity-notify boundary after Closing publication and before wake. The callback removes itself before running work, preventing recursive notification from re-running it. It asserts Closing at entry and verifies both Closed and removal of the handle before returning. Thus it tests a deterministic admissible interleaving rather than hoping a scheduling race happens.

The success test verifies returned admission, durable result, ticket result, terminal destruction, services/handle removal and every relevant transient budget. The delete-failure test checks the original Storage(Unavailable), three deletion attempts, retained charged source and matching ticket failure. The unfinished test independently checks both absent and rejecting handles still yield Worker, then explicitly pumps cleanup and verifies release. Existing `cleanup.rs:43` covers ordinary request saturation without blocking closure; the changed return expression also preserves that Capacity outcome in the completed-race branch by inspection.

Reviewed `linux-red/command.log` and metadata: the unchanged tests on pre-fix source failed precisely the success/storage admission assertions (2 failed, 1 passed; exit 101). No fixture-panic unrelated to the defect appears in that result. Green execution and full gate are root-owned and were not yet locally available when this review was written. No tests, builds, performance workloads or soak were launched by this reviewer.

## Soak diagnostic behavior

`soak.rs:7-34` returns the original generic Result after inspecting only errors with an observed turn. Status and projection-status queries are public reads; their failures are printed as values rather than propagated in place of the triggering error. No native call or state mutation was introduced by the diagnostic layer. The transient finish sequence stays cancel/exit request, wait admission, the same DEADLINE timeout, completion assertions, then forget. Each original error still propagates at its original stage.

`observe_projection` preserves operation admission before waiting, the same timeout, and conversion of the operation error to RuntimeError. Periodic view, checkpoint and resize remain sequential and retain their prior drop/assertion behavior. The public `finish` helper passes no turn, so non-observed callers do not print these error diagnostics. Successful paths do not print diagnostics.

Limitations: diagnostics are selected-stage instrumentation, not exhaustive soak failure attribution; cold-cycle/final finish and existing assertions have not all been instrumented. Stderr writes are best-effort in the sense that write errors are ignored; they are synchronous and are not a bounded-latency logger. The operation timeout itself is unchanged, but failure reporting can take additional time. These limitations do not block this fixture-only change or imply a successful rerun of the failed soak.

## Reviewed SHA-256 identities

| File | SHA-256 |
| --- | --- |
| `crates/application/src/projection/admission.rs` | `c13ae04a2d47d6524bd52ac613ddebdf4ee81800e0c61cdc8969fdb7fd674c8a` |
| `crates/application/src/projection/coordinator.rs` | `1d4a2f5bf79ae25de2a4f1c03a7c213568156fed5627b97b313cd5b3d57181b0` |
| `crates/application/src/projection/worker.rs` | `0c196c99d94ad93c7487e23d256894567389215815450eb77e6b7d726c14e5ef` |
| `crates/application/src/projection/teardown.rs` | `c27f771fcec8f751e35aaea5de0665f98844a13050a300783264b53056bcac22` |
| `crates/application/src/projection/observation.rs` | `9f42977b3af1aed5ba5e8a888b84cf0b85a77127e1e5dd9cba9f6cf2757f15cb` |
| `crates/application/src/projection/tests/close_race.rs` | `882028d05d3d036fac46ea05bb22b3e3205a300e8419a60f6c0b5dd96cc023ad` |
| `crates/application/src/projection/tests/mod.rs` | `c2db84eb26a22bfb77b47878d9a8138bd9c95bb7b663e6121fe49c35d8d7d9a5` |
| `crates/application/src/projection/tests/cleanup.rs` | `0f31b0a37d4e5ddd4d4e873b5d4b7d1e85b0c05be91bfc2077d71a79d7c898bf` |
| `examples/release_support/soak.rs` | `7faea6420df8dbd1b04e35a53341efe9aa5d924bb0a024260357f257a835d128` |
| `examples/release_support/mod.rs` | `b5ed1f5f5230a7fce4a41df12967898d73ac9ed92a8ca10cd166b559c454a866` |
| `docs/verification/close-completion-race/linux-red/command.log` | `54af9548cf26757fa0a5ca0e830b5830f24c548da91380f25273109191b663a9` |
| `docs/verification/close-completion-race/linux-red/metadata.json` | `fa1c6a37c73adf32fcbeae88d45163d91cbc24c0d16ca71a4d7c9d8341549934` |
