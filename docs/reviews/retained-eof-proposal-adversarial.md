# Retained-endpoint EOF proposal: independent adversarial review

## Verdict and scope

**PASS for the isolated proposed production change; no unresolved P1/P2 defect found.** This is not runtime acceptance or authorization to modify the separately owned runtime. Full mechanical/performance gate, other independent reviews, Linux/hosted CI evidence, coverage and release workload requirements remain separate.

Reviewer: `retained_eof_adversarial`, independent of the seven-line production patch author. Reviewed base `a3e0fee3c5974e22486673d94cade302ad9b39ee`, actual isolated `trace-source` reader and lifecycle/session code, the supplied before/after logs and retained-endpoint unit test, and existing real process contracts/pressure fixtures. Final proposal patch SHA256: `55aefd7e61f27bacdbfba1e1d0ff540f1f0c30003a22fc97ff76f4d458103c63`. The new integration test file includes an additional preservation test authored by this independent test reviewer; the separate quality reviewer must also inspect that addition.

## Challenged paths

- Premature endpoint close: the new `Ok(0)` branch requires the existing Acquire workload-exit observation. It does not close on any successful positive read or before workload exit. The nonzero queued-tail path and existing retained-endpoint invariant test are unchanged. A zero-byte read is the actual OS result; the patch does not infer an empty queue from `FIONREAD` or from workload exit alone.
- False EOF and descendants: releasing the retained descriptor is followed by another read, not an immediate EOF return. An active descendant owns its own child endpoint and may continue output. A new independent public-boundary test requires output written after the test observes workload exit to be delivered in full before EOF. The existing silent-descendant contract still requires truncation under its short deadline.
- Race with stop/deadline: `continue` returns to the inner-loop `stop_reader` check before another read. Taking the optional file consumes that ownership once; repeated reads cannot repeatedly close a stale descriptor. Deadline enforcement still belongs to the supervisor. As before, there is scheduling granularity between the clock expiring and `stop_reader` being set; the patch introduces no direct-clock promise. The intentionally expired 250 ms test records truncation.
- Retry/resource pressure: the additional immediate retry can happen only once per session because it takes `retained`. There is no new allocation, worker, mutex, queue or unbounded retry chain. Existing interrupted reads and pre-exit zero-read behavior remain unchanged. EIO handling is not broadened or silently treated as completion while the retained endpoint is present.
- Cleanup/failure: normal reader completion, failed read, stop and callback unwind continue through existing endpoint cleanup and reader completion notification. No resource ownership escapes the reader.

## Findings, fixes and independent test evidence

**P2 acceptance-evidence gap, resolved:** the supplied regression asserted only seven bytes emitted before exit. Existing descendant coverage held an endpoint open but emitted no later data; it could not establish preservation of late descendant bytes. Added `descendant_output_after_observed_workload_exit_is_delivered_before_eof` only in the isolated snapshot's `process_delayed_eof.rs`, with parent coordination before the gate began. No production source was changed by this reviewer.

The test uses a real `/bin/sh` descendant and a release file. The workload prints `initial`, launches the descendant and exits 23. The descendant cannot print `tail` until the test observes exit and creates the release file. The test asserts no drain before release, exact `initialtail`, exit 23, and `DrainOutcome::Eof`. This uses public spawn/events and does not inspect internal flags. Child polling is bounded; a drop guard removes the unique-per-process directory, and backend cleanup precedes directory removal even on panic. This preservation assertion is separate from the authored fail-before/pass-after reproducer.

Executed on the local macOS isolated snapshot:

- `cargo fmt --all` completed.
- `cargo test --locked -p pty-runtime-infrastructure --test process_delayed_eof --no-default-features`: **3 passed**, 0 failed, 0 ignored; 0.72 s. Raw output: `work/coverage-closure/process-pressure-diagnostic/adversarial-delayed-descendant.log`.
- Compared all tracked base files with the isolated snapshot: only production `crates/infrastructure/src/process/io.rs` differs, plus the new `process_delayed_eof.rs` integration test. Regenerated the proposal patch against HEAD; `git apply --check` succeeds in the real worktree.

Reviewed supplied evidence: `regression-before.log` has the EOF test fail as `Truncated` and the short-deadline case pass; `regression-after-contracts.log` has the two original regressions, seven contracts and four pressure tests pass; `retained-endpoint-after.log` has the queued-tail retention invariant pass. The explicitly stale trace-build log is not treated as baseline evidence.

Re-review disposition: the missing descendant preservation assertion is now present and passed; the unchanged seven-line production proposal remains adversarially acceptable in this isolated scope. The separate quality reviewer and parent gate must evaluate the final test addition/hash before combined acceptance.

## Remaining limitations

No full gate, performance cohort or Linux run was executed by this reviewer. The two-second long drain and bounded fixture polling are finite test budgets, not proof against arbitrary scheduler stalls. The new descendant test exercises endpoint lifetime behavior but does not claim to force macOS to return `Ok(0)` while a live descendant owns an endpoint. The supplied delayed callback is intentional slow-consumer stimulus; it establishes the recorded local macOS interleaving, not every historical CI failure's cause. The tiny reproducer alone does not establish arbitrary-sized tail preservation; the existing multi-read retained-endpoint and 128 KiB producer tests supply separate evidence. No proposal has been applied to tracked production source, and the ownership exception remains required for that action.

## Final isolated mechanical-gate evidence review

Independently inspected `work/coverage-closure/eof-proposal-gate/metadata.json` and `command.log` after the parent executed `python3 scripts/gate.py`. Metadata reports exit 0, passed true and unchanged sources, elapsed 112.33 s on macOS 26.6 arm64 with Rust 1.98.1. The raw log ends with `Mechanical gate passed. Specialist reviews and milestone proof remain required.`

Verified all **351** source-file hashes in that gate metadata against both the current isolated snapshot bytes and the frozen `isolated-source-sha256.json`: **zero mismatches** in either comparison. Reconstructed the two-file proposal diff directly from base `a3e0fee3c5974e22486673d94cade302ad9b39ee` and the snapshot; it is byte-for-byte identical to the final patch, still SHA256 `55aefd7e61f27bacdbfba1e1d0ff540f1f0c30003a22fc97ff76f4d458103c63`.

The raw log records all three `process_delayed_eof` tests passing under both `cargo test --locked --workspace --all-targets --all-features` and `cargo test --locked --workspace --no-default-features` (test groups at log lines 2276 and 3015). The retained-endpoint queued-tail unit test also passes in both groups (lines 2184 and 2924). The later `--no-default-features --features event-stream` command targets the root package and is not represented here as a third infrastructure regression execution.

**Final evidence verdict: PASS for the frozen isolated proposal's adversarial review and the inspected local mechanical evidence.** No new P1/P2 finding arose. This reviewer performed read-only evidence/source checks and no new builds. This updates the earlier pending-full-gate limitation; it does not accept or qualify the actual production branch, authorize integration, establish Linux/hosted-CI behavior, or discharge scoped performance, coverage and full release workloads.

## Authorized integration source review

The parent reports the user explicitly extended the ownership exception to apply the reviewed EOF fix. Re-inspected the actual `pty-runtime-program-status` worktree's reader and all three `process_delayed_eof.rs` tests after application. The actual reader SHA256 is `eee5294c8b42105278a5c240c0a72cdfb97c0eeacce7be4d5d1edbf10140c709`; the actual integration-test SHA256 is `f6a1142f8babd373b0fb580116197612756102e89ed2ccf7b32cc0be40a73d99`. Both match the frozen proposal. All 351 source hashes from the passed isolated gate also match the actual worktree, with zero mismatches.

**Integrated source adversarial verdict: PASS, no unresolved P1/P2 finding in this change.** Rechecked one-shot descriptor consumption, stop checks across the new continuation, positive-read tail delivery, descendant ownership, unchanged EIO/pre-exit handling, and test sequencing. The preservation test's release file is created after observed workload exit; exact output and EOF assertions prevent a premature successful drain from becoming a false green. The deliberate callback delay remains a bounded regression stimulus, not performance evidence. No new allocations, persistent resources or unbounded retries were added.

The earlier pending-authorization statements describe the proposal stage and are superseded by this explicit integration authorization. The integrated gate is still running at this source-review point; mechanical evidence will be recorded separately when complete. Coverage and scoped performance must be rerun on the actual integrated source. Prior failed cohorts remain failed evidence, and this source verdict does not imply complete runtime readiness. This reviewer performed no builds, tests or source changes during integrated review.

## Prospective controlled-cohort driver review

Reviewed ignored `work/coverage-closure/run_eof_controlled.py` SHA256 `fa0ba83c2f60972d27c46f8344f95226d7616ce548c6f4fb7add3647b5c2fdb6` and `retained-eof-integrated-performance.md` before execution. The original baseline, five alternating pairs, both 1/16-session 10% median thresholds, no sample replacement, and explicit separation of processing regression guard from EOF benefit are appropriate. No cohort was executed in this review.

Two **P2 measurement-evidence findings** block executing this driver as the declared qualification:

1. At lines 53–70, recording every initially globbed executable/helper and later hashing only those paths does not identify Cargo's actually selected test executable. Timed launchers invoke Cargo again; a newly selected/generated artifact path could escape this inventory while the old files remain unchanged. Bind preparation to Cargo's selected executable, verify each trial actually uses that identity, and reject unexpected artifact inventory changes/recompilation during measurement.
2. At lines 100–116, missing/malformed metadata or records raises an uncaught file/JSON/key error, stopping the fixed cohort without its declared invalid-trial summary; empty arrays can also break median calculation. Preserve explicit invalid-trial reasons, continue the remaining declared trials without replacement when source/artifact identity remains valid, and produce a failed summary for incomplete data. Identity drift is a distinct reason to halt rather than intentionally measure changed inputs; declare that invalid-cohort safety stop in advance.

Both findings were sent to the coordinating parent and driver author for correction and re-review before measurement. These findings concern prospective measurement collection, not correctness of the seven-line runtime change. Earlier failed performance cohorts remain failed.

## Integrated mechanical evidence confirmation

Independently inspected actual-worktree `work/coverage-closure/eof-integrated-gate/metadata.json` and `command.log`. The completed gate reports pass/exit 0 in **124.216 s**, sources unchanged, macOS 26.6 arm64 and Rust 1.98.1. All **351** recorded source hashes match actual current files and the complete source-file hash map equals the earlier isolated gate's map. There are no source mismatches.

Raw log lines 2239–2246 and 2978–2985 show all three delayed-EOF/late-descendant tests passing in the all-features and no-default-features workspace runs. The retained-endpoint queued-tail invariant passes in both runs (lines 2147 and 2888). The log ends with the mechanical gate pass at line 3467. Thus the mechanical evidence now covers the authorized actual integrated source; it is no longer merely proposal-only evidence.

**Integrated source and inspected mechanical evidence remain PASS with no runtime-change P1/P2 finding.** The prospective measurement-driver P2 findings above are separate and still require resolution before its cohort runs. Actual strict coverage is independently being rerun; completed prior coverage/performance failures are not erased. No performance or full-release readiness claim follows from this mechanical pass. This confirmation used read-only evidence inspection, with no builds or source changes by this reviewer.

## Prospective driver re-review and actual coverage evidence

Re-read final unexecuted ignored driver SHA256 `de8defd02def72690d55966776d3c583afd28dd2e475dded7a20e82f42bc24ac` and updated prospective plan. **Both measurement-driver P2 findings are resolved; scoped prospective review PASS.** Cargo JSON preparation selects exactly one runtime test executable, saved alongside complete runtime/helper inventory hashes. Every trial requires the corresponding resolved `Running` path, rejects compilation/checking/dirty output, and checks the whole inventory plus native hashes before and after. New/missing paths can no longer hide behind unchanged old hashes.

Missing/malformed evidence and launcher failures now produce explicit per-trial invalid reasons and continue the fixed remaining schedule without replacement. Qualification requires ten valid executions and five rows per configuration/session count; incomplete/invalid data produces a failed summary without qualifying medians. Source/artifact drift explicitly aborts the cohort as invalid, a stopping policy declared prospectively rather than selected from timing results. Inspected the author's synthetic-validator log; no benchmark was executed by this reviewer. This approval concerns collection methodology, not a performance result.

Actual integrated strict coverage metadata at `work/coverage-closure/eof-integrated-coverage/metadata.json` reports **FAIL** after **118.958 s**, unchanged sources. Independently hashed all 351 recorded files against the current worktree: zero mismatches. All execution phases passed (workspace all-features, raw, event-stream, and separate helper tests); both 100% coverage reports exited 1. Independently read the LLVM JSON totals:

| Scope | Lines | Functions | Regions |
| --- | --- | --- | --- |
| Workspace | 9,959 / 10,535 (94.5325%) | 1,029 / 1,104 (93.2065%) | 14,611 / 15,737 (92.8449%) |
| Separate helper | 476 / 1,478 (32.2057%) | 62 / 109 (56.8807%) | 747 / 2,274 (32.8496%) |

The reader file is 155/181 lines, 8/8 functions and 200/236 regions. The export marks the added exit guard/release/continue lines as covered, but exact counts are not reliable: line 114 records 46, lines 115–116 contain saturated `9223372036854775807` counts, and line 117 records 55. These are not credible literal execution frequencies and must not be presented as such; this review did not determine the instrumentation/merge cause. The named functional regressions remain independent behavioral evidence. Branch/MC/DC counts of zero mean unsupported measurement here, not complete branch coverage.

**Readiness remains FAIL.** The 100% requirement is unmet; native bridge, upstream native dependency, other platforms and build tooling remain outside this command's scope. Previous failed performance cohorts remain failures. New cohort execution/results and remaining platform/full-release evidence must be assessed separately.

## Coverage integrity diagnostic: established facts and open cause

A read-only comparison with `coverage-msrv-final/workspace.json` establishes that the prior export already had one saturated function: the `SessionContext::update_and_notify` instantiation used by `poisoned_context_drop_releases_owned_quotas_and_diagnostic_gauges`. Its waker-loop regions at `context.rs:178–179` report `9223372036854775807`. That fixture deliberately panics inside the change closure at line 163 before the waker loop, so exceptional control flow is a plausible contributor to invalid derived counts. This is a pre-existing coverage-integrity limitation, not a new EOF defect.

The prior reader instantiations contain no saturated regions. The actual integrated export adds three saturated `read_loop` instantiations, beyond the same context anomaly. This is visible in function-level regions, not only aggregated line rendering:

- One reader instantiation has exit-guard count 3, false/end count 6, and saturated release/continue counts (a negative inferred difference of −3).
- Another has initial retained test count 75 but the no-retained EOF return count 78; the following region/guard/release counts saturate.
- A third has exit-guard count 46, false/end count 49, and saturated release/continue counts (again −3).

Both prior and current separate-helper exports have zero saturated function regions. The logs completed test phases successfully; source hashes match and run-owned target/profile roots were used. This is not evidence that source changed during the run or that the functional tests failed. It also is not evidence that every exported covered line really executed: **the earlier statement that the new lines are covered is only a description of the anomalous export, not valid measurement proof.** Workspace line/region percentages and comparisons must be labeled **reported export totals with an instrumentation anomaly**, not authoritative coverage gains or proof of changed-line completeness. Preserve the raw exports and failed readiness result. Separate-helper totals retain their separate narrow scope; absence of saturation alone does not establish full measurement validity.

Read-only tool inspection found `cargo-llvm-cov 0.6.16`; the actual infrastructure fingerprint records `-C instrument-coverage`, `--cfg=coverage`, and `--cfg=trybuild_no_target`, without atomic counter-update instrumentation. Rust 1.98.1's `-C llvm-args=--help-hidden` lists `--instrprof-atomic-counter-update-all` as “Make all profile counter updates atomic (for testing only).” Lost updates in concurrently executed non-atomic profile counters are a plausible explanation for reader edge-count inconsistency. Counter-mapping/exceptional-flow issues are also not ruled out. No cause has been proven from these exports, and no production behavior defect follows from this observation.

Proposed, not executed by this reviewer: a coordinated isolated same-source diagnostic with fresh profiles/target and atomic profile instrumentation, exercising the existing delayed-EOF, pressure and retained-endpoint tests, then comparing reader function-level conservation and saturation. `scripts/coverage.py` clears RUSTFLAGS, so merely setting that environment variable around it would not establish the intended experiment. Do not alter production behavior or discard earlier exports to obtain a green percentage. Even clean atomic reader counters would not by themselves resolve the pre-existing panic-instantiation anomaly or full readiness gaps.

Also inspected `eof-integrated-msrv/metadata.json`: the explicit `cargo +1.85.0 test --locked --workspace --all-targets --all-features` command passed in 53.570 s with unchanged sources and all recorded hashes matching actual files. Its generic `rustc` metadata field reports the default 1.98.1 toolchain, so that field alone is not proof of the selected MSRV; the explicit command names 1.85.0. MSRV success does not change the coverage readiness FAIL.

## One authorized paired profile-counter diagnostic

Executed exactly one coordinated default/atomic diagnostic pair after the performance cohort had finished, preserving evidence at `work/coverage-closure/eof-counter-diagnostic`. Both arms used unchanged actual source, independent fresh target/profile directories and the same explicit uninstrumented helper from the completed strict-coverage run. Each arm ran only the existing delayed-EOF and pressure integration targets (7 tests) and the retained-endpoint unit filter (1 test), then exported JSON. Metadata retains commands, durations, source/helper identity, and the exports; `comparison.json` retains the reader region comparison and actual compiled rustflags.

The first default retained-filter invocation was rejected by Cargo LLVM-Cov's argument parser (`unrecognized subcommand retained_endpoint_tests`) before executing any test. Its command/log remain preserved. Corrected only the CLI separator to `-- retained_endpoint_tests` and continued; the completed integration tests were not repeated. Both arms' actual test runs and JSON exports passed. Final source/helper hashes are unchanged. Atomic-arm fingerprints confirm `-C llvm-args=-instrprof-atomic-counter-update-all`; default fingerprints omit it.

**Result: INCONCLUSIVE for the lost-counter-update hypothesis.** The default arm did not reproduce any saturated function regions, and neither did the atomic arm. Moreover, the new guard/release/continue regions at reader lines 114–116 report zero in both arms; this reduced run did not measure those regions executing. Each arm has reader counts consistent with all observed `Ok(0)` returns occurring after the retained endpoint was already absent (20/20 in the integration reader and 1/1 in the retained-unit reader). Passing the behavioral regression fixture does not guarantee that every execution takes the newly added branch.

Therefore this pair neither proves that atomic instrumentation repairs the full-matrix anomaly nor validates the new branch's coverage. It does not replace the full failed strict-coverage run, establish 100% coverage, resolve the pre-existing context-panic anomaly, or justify another run until favorable. Original exports remain intact. No production code or tracked script was changed. **Coverage readiness remains FAIL, with measurement-integrity limitations explicitly open.**
