# Coverage closure adversarial review

Review date: 2026-10-11. Reviewer: independent adversarial agent (`code_quality_gate`), not an author of this batch.

Scope: uncommitted test and native coverage-driver additions over `5c7434ac2c129870dc6b1adbfa9a5d493b444cdd` in `pty-runtime-program-status`. Reviewed domain/public boundaries, application runtime/projection and owner boundaries, infrastructure checkpoint/process/scheduler tests, facade composition tests, guardian units/startup tests, native boundary shim/contracts and native coverage driver.

## Decision

No P1/P2 code finding in the inspected batch. Final readiness acceptance is **pending**, not passed: the full mechanical gate and final Rust workspace/helper coverage matrix have not been supplied to this reviewer. Re-review is required after the final freeze and those results. This review does not qualify a release milestone, Linux behavior, or the upstream Zig engine.

## Adversarial checks

- Production edits in this batch only connect `cfg(test)` modules; no production branch is removed or behavior altered to obtain coverage. The native driver expands its real-engine test list; it does not shrink the four-file own-C denominator.
- The native injection wrappers affect separately compiled test objects. Added compression and decoder-budget assertions check the bridge's exact result translation and live-allocation cleanup; they are explicitly synthetic evidence, not proof that the native engine naturally reaches those states.
- Independent inspection of `work/coverage-closure/native-complete-evidence/coverage-export.stdout.txt` confirms all four own-C files: 249/249 lines, 23/23 functions, 356/356 regions, 198/198 branch sides. All nine recorded contract executables exit zero. Current changed native driver, shim, contract, and runner hashes match `source-manifest.json`.
- Guardian startup rejection executes a fresh child and uses a kill/wait guard. New unit tests do not call session/group creation or group-signal operations in the multithreaded test host. Owned descriptors and socket pairs have scoped cleanup. The injected cancellation test has no live workload and does not tick into signaling.
- New application poison/counter-exhaustion tests deliberately create internal boundary states; they prove the local recovery and quota accounting assertions, not real process/native cleanup. Port doubles likewise establish application orchestration only. Actual filesystem, socket, and native tests provide the respective boundary evidence.
- Coverage remains an additional metric, not proof of all behavior. The guardian continuous-profile experiment is explicitly rejected by its driver because of saturated region expressions; this review does not accept those counters as valid helper completion evidence. Normal Rust workspace/helper and platform gaps remain open until independently measured.

## Independently executed checks

- `cargo test --locked -p pty-runtime-domain --test public_boundaries`: 7 passed.
- `cargo test --locked -p pty-runtime-application --lib boundary`: 11 passed (includes matching pre-existing tests).
- `cargo test --locked -p pty-runtime-application --lib construction_boundaries`: 2 passed.
- `cargo test --locked -p pty-runtime-application --lib coverage_boundaries`: 5 passed.
- `cargo test --locked -p pty-runtime-application --lib lifecycle::tests`: 3 passed.
- `cargo test --locked --manifest-path helpers/guardian/Cargo.toml --bin pty-runtime-guardian`: 17 passed, including seven new link and four new public-boundary tests.
- `cargo test --locked --manifest-path helpers/guardian/Cargo.toml --test startup`: 2 passed.

The application test inventory changed during review as owner-boundary tests were added. Their source has been inspected; a frozen final run and source identity are still required. No production or test files were edited by this reviewer.

## First frozen coverage result

Independently inspected `work/coverage-closure/coverage-first/metadata.json`, `workspace.json`, and `helper.json`. Source identity remained stable. Every behavior phase exited zero; the workspace and helper threshold report phases both exited one. Readiness is **BLOCKED by the requested coverage threshold**, not passed:

| Scope | Lines | Functions | Regions |
| --- | --- | --- | --- |
| Rust workspace | 9982/10566 (94.4728%) | 1035/1107 (93.4959%) | 14721/15853 (92.8594%) |
| Guardian helper | 476/1478 (32.2057%) | 62/109 (56.8807%) | 747/2274 (32.8496%) |

Rust branch counts are zero because this run does not instrument branches; zero does not mean full branch coverage. The native own-C 100% result cannot fill these separate denominators. No excluded production errors or weaker thresholds were introduced in this batch.

The four final application owner-boundary tests were independently rerun and passed. A separate DDD reviewer is challenging the private-state fault setup against the public-surface testing rule. This adversarial review establishes that those assertions are local white-box fault checks, not public reachability evidence; it does not override that review. Final code acceptance remains held until that finding is dispositioned and resulting changes are re-reviewed.

## Review-fix re-review

Reviewed the frozen fixes removing private maximum-counter/poisoned-lock tests and the entire lifecycle test module. Replacement owner-boundary cases use Runtime and existing process/repository ports for observer quota rollback/reacquisition and shutdown racing an admitted spawn. The concurrent case establishes both entries with channel handshakes, uses a five-second release receive bound, and joins scoped threads. No OS child or process-group operation is introduced. It proves late admission is refused and the late-bound process is canceled; it does not alone prove shutdown waits for every admitted operation before returning.

The descriptor-adoption test now clears FD_CLOEXEC and checks the clear state before adoption, so success cannot be inherited from UnixStream::pair. Independently reran all six owner-boundary tests and the corrected descriptor-adoption test: all passed.

**Adversarial code/test re-review: PASS for this corrected batch; no unresolved scoped P1/P2 code findings.** Whole readiness remains blocked by unmet coverage thresholds. The earlier first-coverage numbers are historical evidence for the preceding test set and must not be presented as the final corrected-source measurement. Final mechanical and coverage results remain required.

## Final corrected-source mechanical and Rust coverage evidence

Independently verified `gate-final/metadata.json` and `coverage-final/metadata.json` under `work/coverage-closure`: the source-file hash maps match exactly, both runs report unchanged source, and the mechanical gate passed in 128.319 seconds. Coverage completed in 122.093 seconds; every behavior phase passed, while both threshold reports failed.

| Reported aggregate | Lines | Functions | Regions |
| --- | --- | --- | --- |
| Workspace | 9941/10528 (94.4244%) | 1029/1104 (93.2065%) | 14581/15724 (92.7309%) |
| Helper | 476/1478 (32.2057%) | 62/109 (56.8807%) | 747/2274 (32.8496%) |

These are tool-report aggregates. The workspace export includes test-only support files such as `runtime/boundary_tests.rs` and `runtime/owner_boundary_tests.rs`; do not relabel the aggregate as a strictly production-only percentage. Deleting the challenged white-box fixtures changes this denominator without removing production behavior. Retain the separate production source inventory when evaluating readiness. Rust branch coverage remains unmeasured, not 100%.

**Final disposition for the corrected test batch: adversarial PASS. Readiness disposition: BLOCKED**, because the required 100% Rust/helper coverage is unmet. Final native snapshot and controlled performance reruns were still pending at this update; no whole-project 100% or release qualification is certified.

## Conventional helper profiling diagnostic

An isolated conventional-profile experiment under `work/coverage-closure/guardian-conventional-*` used a linked compile-time `%p` filename, ordinary atomic instrumentation, no continuous mode, no atfork hook, and unchanged helper source. All 17 units and 20 actual protocol scenarios passed; no saturated regions were found. However, the actual helper starts left 20 empty profiles. Eight nonempty actual profiles belong only to the fixture executable. Unit-only and merged helper exports are identical (633/1682 lines, 65/121 functions, 1123/2730 regions, including test support); actual-only covers zero helper code. Sentinel normal retirement also kills its own group, so it does not flush on normal completion. This disproves the proposed collection improvement for these cases; it is not 100% or new readiness evidence. See the experiment's SUMMARY.md and comparison.json for raw evidence and denominator limits. No tracked production or measurement script was modified.

## Scanner plain-chunk optimization review

Reviewed the three-line early return in `Scanner::scan`: `plain_chunk` requires Ground mode and absence of ESC. In exactly this case the previous loop's Ground search immediately found no ESC and broke without changing mode, body, length, escape state, mutations or query offsets. Empty chunks also remain no-ops. Escape/OSC/other-string continuation states cannot take the shortcut, including chunks containing only a terminator/cancellation and no ESC. Ground chunks with later controls take the unchanged offset-based loop. No behavioral correctness blocker found for the optimization.

Independently ran `cargo test --locked -p pty-runtime-domain` after the optimization: 40 unit, 17 acceptance, 5 coverage, 9 public-boundary and 1 replay-model tests passed (72 total). Existing tests explicitly exercise split query terminators and every control-string chunk size. Additional test changes were active during this run, so this is scoped scanner evidence rather than final frozen-batch acceptance. Performance benefit still requires its separate controlled measurements.

## Frozen follow-up batch review

Reviewed the final changed/untracked code, including the two new public ProjectionPolicy boundary cases, later-transfer-observer replay isolation, and projected backend-rejection rollback. The code inventory contains 24 non-document changed/untracked files; SHA-256 of the sorted compact JSON path-to-content-hash map is `5b612fe459141cc0fa73a3c0b4323b7995218b22d91cf4cc7c2d336bc076b111` over base `5c7434ac2c129870dc6b1adbfa9a5d493b444cdd`.

The deterministic CloseDriver runs work through the existing scheduler/capacity ports, releases its handle-list lock before calling work, bounds rollback pumping to sixteen turns, and retains weak owner references. Its assertions verify both native-model destruction and release of every reservation, then reuse the sole runtime slot and same ID. The transfer test keeps the first observer pinned while rejecting a cursor before the second observer's boundary, then checks the exact later event. The domain overflow case enters the large value through the existing public restoration-progress API and checks failure atomicity. No new OS process/signal manipulation or unbounded background work was introduced.

Inspected `followup-public-tests` logs: 9 domain-boundary tests, 1 backend-rollback test, and 1 transfer-isolation test passed. No additional builds/tests were started during the parent's gate/performance activity. **Adversarial code review PASS for the frozen follow-up batch; no unresolved scoped P1/P2 findings.** Mechanical/coverage evidence for this exact batch remains pending and the 100% readiness threshold is not waived.

Profiling interpretation clarification: continuous-profile saturated expressions include guardian loop regions 144–148 and 159–176 around terminating cleanup. Incomplete active-loop counters after asynchronous termination are consistent with these observations. Fork corruption is not established as the cause; the cause remains unresolved. Conventional profiling proves missing flushes for the exercised helper roles, not the cause of the continuous-expression saturation.

## Final scanner-batch evidence and disposition

Independently verified `work/coverage-closure/gate-scanner/metadata.json` and `coverage-scanner/metadata.json`. Both source maps match exactly, both runs report unchanged sources, and every recorded source hash still matches the current file. The mechanical gate **PASS** took 132.604 seconds. Coverage **FAIL** took 117.823 seconds: all behavior/build phases returned zero; only `workspace-report` and `helper-report` returned one for unmet thresholds.

| Tool aggregate | Lines | Functions | Regions |
| --- | --- | --- | --- |
| Workspace | 9955/10531 (94.5304%) | 1029/1104 (93.2065%) | 14604/15729 (92.8476%) |
| Helper | 476/1478 (32.2057%) | 62/109 (56.8807%) | 747/2274 (32.8496%) |

Rust branch counts remain 0/0 (unmeasured). Aggregate/test-scaffolding caveats above still apply. **Final adversarial code disposition: PASS. Coverage readiness disposition: FAIL/BLOCKED.** No whole-project 100% claim or release qualification is approved. The parent is separately running native coverage and the preplanned performance cohort; neither can replace the failed Rust/helper threshold. No additional builds were run by this reviewer during validation.

## Rust 1.85 compatibility P2 re-review

Hosted MSRV testing found P2: the new CloseDriver test used a let-chain unsupported by the declared Rust 1.85 compiler (`admission_rollback.rs:378`). Inspected the retained hosted error in `/tmp/pty-coverage-msrv-failure.log`: rustc 1.85.0 rejects it with E0658. This compatibility blocker was missed in the preceding source review and was caught by the compiler matrix.

Independently reviewed the replacement: an early continue for `closed || !pending.swap(false)` followed by `let Some(work) = upgrade() else { continue; }`. Short-circuit order is identical: closed handles do not clear pending; only open handles attempt the swap; only previously pending handles upgrade; only successful upgrades run work. There are no production changes or altered callback/lock lifetimes. `work/coverage-closure/msrv-fix-focused.log` records the focused rollback test passing with the corrected source (1 passed).

**P2 code fix resolved; adversarial re-review PASS for this correction.** Full local/hosted MSRV and refreshed stable gate/coverage remain separate validation requirements. Earlier performance failure is not cleared by this test-syntax fix. No builds were run by this reviewer.

## Final post-MSRV evidence validation

Independently compared `msrv-fix-matrix`, `gate-msrv-final`, and `coverage-msrv-final` metadata under `work/coverage-closure`. All three source maps are identical, each reports unchanged sources, and each recorded file hash matches the current file. For an explicit reproducible identity, SHA-256 of the sorted compact JSON source-file hash map is `5b40f522d5b4281f409c02b1493a093b59bff16334a0ceb82de391411f30e50e`.

MSRV matrix: **PASS**, 67.307 seconds. Mechanical gate: **PASS**, 51.344 seconds. Coverage: **FAIL**, 122.943 seconds; only the workspace/helper threshold-report phases failed, while all behavior phases passed. Final workspace counts are 9955/10531 lines, 1029/1104 functions, and 14603/15729 regions (92.8412% regions). Helper remains 476/1478 lines, 62/109 functions, and 747/2274 regions. Rust branches remain unmeasured.

**Final adversarial code review: PASS; MSRV P2 resolved. Overall readiness remains BLOCKED by the unmet coverage target and previously reported performance failure.** A commit/push records this reviewed progress; it does not establish 100% coverage or release/readiness acceptance. No builds were run for this validation.
