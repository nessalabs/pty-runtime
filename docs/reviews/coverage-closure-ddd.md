# Coverage closure: independent domain and boundary review

Reviewed the working changes over `5c7434a`, including untracked tests, on 2026-10-11. The SHA-256 of the sorted changed source paths and contents (excluding review documents) was `575967b7155655f26e9ae396087e077972de5f59072a5ecc64295004d232dd90` across 23 files. This reviewer did not author the coverage-closure tests. No heavy test or measurement was launched during the concurrent coverage run.

## Result

**Scoped DDD/boundary review passed after fixes: both P2 findings below are independently resolved.** No production dependency-direction defect was found. The added runtime wiring is conditional test-module inclusion; the production Rust and C algorithms are unchanged. Fixing the findings requires changing tests, not weakening production checks or exposing new test-only state.

### P2: Private counter assignment and mutex poisoning do not prove public boundary behavior

`crates/application/src/runtime/lifecycle_tests.rs:9-11` assigns private admission counters directly. In the spawn-counter case it creates `sequence == 41` with `spawns == usize::MAX`. Normal admission increments sequence on every successful spawn admission and decreases only the outstanding-spawn count when a guard drops, so this state violates `spawns <= sequence`. It is not a difficult-to-reach valid runtime state. The sequence-exhaustion case and `boundary_tests.rs:64` setting `next_watcher = u64::MAX` represent mathematically possible high counter values, but obtain them by writing private state rather than exercising the real boundary.

`lifecycle_tests.rs:43-46` and `boundary_tests.rs:46-51` likewise manufacture poison by locking internal mutexes and panicking in the test. The calls at `boundary_tests.rs:80`, `:104`, and `:140` exercise recovery from that injected internal condition, not an established public collaborator failure. The test names and module description call these acceptance or collaborator boundaries without a real external failure path.

The coding skill requires tests through the public surface and no test-only routes to unreachable state. The user's coverage target does not override that requirement. These tests must therefore be removed or rewritten, rather than treated merely as a prose caveat while accepting this batch.

Required fix:

- Remove `exhausted_identity_or_spawn_counter_changes_neither_counter` and the private-poison admission test. Preserve the shutdown admission/guard-release test, but assert its observable admission/shutdown behavior instead of reading private counters where possible.
- Keep the legitimate observer-capacity portion of `local_observer_and_identifier_exhaustion_return_the_shared_reservation`; remove the assignment and manufactured identifier-exhaustion portion.
- Remove or rewrite the three new poisoned-context tests through an existing external port only if a real public call can induce the relevant condition. Do not add a poison setter, configurable production counter, visibility widening, or a new test-only constructor. If no such path exists, leave the defensive branches honestly uncovered.

Existing tests outside this diff are not retroactively reviewed or approved by this finding. The public `ProjectionPolicy::admit_output(usize::MAX, ...)` counter test in `crates/domain/tests/public_boundaries.rs` is different: it exercises an already exposed domain operation, checks an actual typed outcome and leaves private state untouched.

### P2: Descriptor adoption test starts with close-on-exec already set

`helpers/guardian/tests/public_boundaries.rs:36-44` obtains a descriptor from `UnixStream::pair`, which creates close-on-exec sockets, and then asserts that `os::owned` sets `FD_CLOEXEC`. That assertion still succeeds if the flag-setting operation in `os::owned` is removed. It tests preservation of an existing flag, not the stated adoption contract.

Required fix: explicitly clear `FD_CLOEXEC` on the fixture-owned descriptor, assert the cleared precondition, call `os::owned`, and then assert the flag is set and ownership is retained. Keep the existing invalid-descriptor and peer-EOF checks. A mutation check of the flag-setting operation should demonstrate that this assertion fails; any mutation must be made in an isolated copy and production behavior restored unchanged.

## Boundaries that are sound

- Domain tests use standard-library values and typed domain failures. No native representation or infrastructure dependency enters domain or application code.
- The application factory, scheduler, repository and process doubles implement existing I-prefixed ports. Their assertions establish orchestration, admission rollback and ownership behavior; they are not evidence of real PTY, storage or native-engine behavior.
- Checkpoint adapter tests use the real protector and filesystem store through their ports. Lifetime/descriptor mismatches and capacity failures stay typed boundary outcomes, and failed admission preserves foreign filesystem content.
- Guardian links use real Unix sockets and retain endpoint identity, bounded delivery and one-sided failure isolation. Startup tests execute the real helper in a child process and check CLI admission failure; they do not claim supervision success.
- The new C shim cases change test-linked external returns only. Compression pending/completed and decoder-ready out-of-memory cases test our bridge's result mapping, not Ghostty's implementation of those conditions. The formatter success case runs the real formatter.

## Evidence and readiness

The own-C evidence in `work/coverage-closure/native-complete-evidence` reports 249/249 lines, 23/23 functions, 356/356 regions and 198/198 branch sides. All seven real native Rust contract executables, the allocator contract and the explicitly labelled synthetic boundary contract exited successfully. I verified that the four changed native test/coverage source hashes match that evidence's frozen source manifest.

Those measurements cover the four owned C bridge files only. Rust, the pinned Zig engine and other platforms are not included in that denominator. Instrumentation coverage is not proof that every fault is naturally reachable. The workspace/helper coverage run, full mechanical gate and scoped performance check were still pending during this review; no whole-runtime coverage or release-readiness pass is claimed here. Both P2 findings require changed tests, affected reruns and independent re-review before this review can become a scoped pass.

## Descriptor-adoption re-review

**Resolved independently on 2026-10-11.** The corrected `helpers/guardian/tests/public_boundaries.rs` has SHA-256 `b33978fb33ca3817a85fd84dfd83e6a75a0d3df657a4a60fa01599e066add11b`. It retains `socket` ownership while clearing and checking the flag, then transfers ownership to `os::owned` and checks both `FD_CLOEXEC` and peer EOF after drop. The invalid-descriptor assertion remains.

I compared the current test byte-for-byte with the isolated mutant copy. The only production-source difference in that copy is `os::owned` using `F_GETFD` instead of `F_SETFD/FD_CLOEXEC`; the working production file remains unchanged from `5c7434a`. `work/coverage-closure/cloexec-mutant-final.log` records the corrected test failing specifically at the missing-flag assertion. `cloexec-correct-final.log` records its passing result and `helper-clippy-final.log` records a successful strict helper lint run. I independently reran the single descriptor-adoption test against the current helper; it passed. This is a fixture correction with demonstrated regression sensitivity, not a production behavior change.

At that first re-review, the private-state fixture finding remained open pending its separate changes. Coverage, full mechanical and performance gates remain separate readiness requirements.

## Private-state fixture re-review

**Resolved independently on 2026-10-11.** The changed-source digest is now `d95964f794d78b75be2b2ea4212e5c48cee15831fd688c6bfeb40393be1e5c5a` across 21 source files, using the same path/content hashing scope as the initial review.

I verified that `runtime/lifecycle_tests.rs` is absent and `runtime/lifecycle.rs` has no difference from `5c7434a`. The new boundary tests no longer assign MAX counters, directly lock private runtime state to poison it, or claim those manufactured states as acceptance coverage. The deliberately unexercised defensive overflow and poison-recovery paths remain explicit coverage limitations.

The replacement observer test in `runtime/owner_boundary_tests.rs` constructs the public Runtime using existing process/repository ports. It observes local refusal, the shared reservation count, successful reacquisition after drop and final release entirely through Runtime and Session operations. The replacement shutdown test blocks an existing external backend's spawn on a bounded channel, synchronizes with backend shutdown, verifies new admission is refused, releases the previously admitted spawn and observes cancellation of the returned process. It does not introduce production visibility, counters or fault setters. These assertions establish application orchestration, not native process supervision.

I independently ran the six `runtime::owner_boundary_tests` and four remaining `runtime::boundary_tests`; all ten passed. The supplied `work/coverage-acceptance-followup/review-fix-tests.log` records all 150 application tests passing, and `review-fix-clippy.log` records the strict application lint pass. The author correction is recorded separately in `coverage-closure-application-fix.md`; this independent verification, rather than that author statement, resolves the P2.

There are no remaining P1/P2 DDD or boundary-contract findings in this coverage-closure source scope. This is a scoped review pass only. The final frozen-source mechanical gate, coverage remeasurement, performance checks, other specialist reviews and cross-platform/release workloads remain separate acceptance requirements; no aggregate 100% or release-readiness claim follows from this review.

## Final scanner and adapter scope re-review, 2026-10-11

**Scoped DDD/dependency-direction PASS; no unresolved P1/P2 findings in this scope.** This follow-up reviews the actual three-line production scanner guard plus the helper, native-contract, coverage-runner and infrastructure-test batch. The 14 reviewed paths have SHA-256 `6aefa62d8ca55a31bb33b441423a318795d66683d363bf3f60f7f2d16eed943b` using sorted path, NUL, file contents, NUL concatenation. Unlike the earlier review snapshot, this candidate does change production scanner control flow; the earlier statement that production algorithms are unchanged is historical.

The reviewer authored the latest public-boundary domain/application test additions. Those tests, including the runtime backend-failure slot-reuse test, are explicitly excluded from this independent review and require the other reviewers' assessment. No production implementation, helper/native/infrastructure test or scanner change in this follow-up scope was authored by this reviewer.

Reasoning and findings:

- `Scanner::scan` now returns when its existing `plain_chunk` predicate says the scanner is Ground and the input contains no ESC. Under those exact conditions the former loop either never ran (empty input) or searched unsuccessfully for ESC and broke, changing neither scanner state nor mutation/query vectors. The shortcut preserves that behavior, including ignored BEL, cancellation and high bytes in Ground. In Escape, OSC and generic-string continuation the predicate is false regardless of input, so chunk continuation, terminators, cancellation and query offsets remain on the existing path.
- The scanner remains the owner of its continuation state and the fast-path decision. Domain adds no dependency, unsafe operation, infrastructure concept, duplicate parser state or strategy abstraction. `ProgramStatusStore::scan` still validates chunk size, generation/revision bounds and constructs the identity-bearing plan before calling the scanner; commit still validates identity/generation and advances generation. The optimization does not bypass admission or transaction rules. An ESC-bearing Ground input can be scanned twice; the asymptotic bound remains linear, and performance acceptance must rely on measurements rather than treating this as a universal speedup.
- Guardian's production-file changes only include test modules behind `cfg(test)`; its CLI integration target exercises the real helper. Links assertions cross actual Unix sockets and preserve typed frame/end-point behavior. Descriptor adoption establishes a missing CLOEXEC precondition before checking adoption, so the earlier regression-sensitivity finding stays resolved. The startup fixture preserves only the profiler output destination after clearing environment; it makes no claim to exercise successful workload environment policy or process supervision.
- Infrastructure tests invoke concrete adapters through their established APIs and assert typed rejection, quota cleanup, descriptor/environment admission and ownership of filesystem content. Native or filesystem representations do not move into domain/application. The empty encrypted-store fixture tests the store's opaque-byte responsibility; authentication remains the protector's separate contract. The scheduler test checks rejection after shutdown with a retained work owner, so an empty receive result cannot merely be a disconnected producer.
- Native fault shims map external engine results at the existing C bridge seam and remain test-linked. The injected pending/complete and decoder budget results establish bridge conversion contracts, not naturally occurring engine faults. The real formatter success assertion complements those synthetic cases. Expanding the executable list in the coverage runner adds real contracts without reducing the four-owned-C-file denominator, changing completeness thresholds or omitting failing outcomes.

Evidence inspected without launching builds or overlapping the parent's final run: current source/diffs; the corrected descriptor test and its existing passing/mutant evidence described above; `work/coverage-closure/scanner-experiment/candidate.tests.log` (domain test run passes); the scanner experiment and planned controlled integrated cohort recorded in `coverage-closure-performance.md`; and the runner's explicit failure/completeness checks. Existing own-C evidence remains limited to the owned C bridge. This source review does not certify final-source test execution, full Rust coverage, scheduler races, real supervision, cross-platform operation or integrated performance. The parent's final mechanical gate, coverage evidence, separate adversarial review and fixed performance cohort remain required acceptance evidence.

## Parent-authored Rust 1.85 compatibility correction

**Scoped dependency/contract DDD PASS.** Reviewed only the parent's rewrite of `CloseDriver::wait_after` from an `&& let Some(work)` chain into a short-circuit rejection guard followed by `let Some(work) = ... else { continue; }`. Current `admission_rollback.rs` SHA-256: `5b8427e0aa78704bd9885c946adedb3c6b4b00baa236fc14b5f5f161d109de16`. This is not independent approval of the original test, which this reviewer authored.

Evaluation order and effects are preserved: a closed handle does not consume its pending flag; an open handle consumes the flag once and proceeds only if it was pending; only then is its weak scheduled-work reference upgraded. A missing work owner skips execution, and successful upgrades invoke the same scheduling-result match. The change introduces no production visibility, dependency, new state, alternate ownership or domain-rule duplication. It retains the existing external scheduler/capacity ports and their test-only orchestration boundary.

The supplied `work/coverage-closure/msrv-fix-focused.log` records the affected test passing with 151 filtered tests; the parent reports this invocation used Rust 1.85. This reviewer launched no build. The original test's independent quality/adversarial assessment and completion of the full MSRV matrix remain separate requirements; this entry approves only the parent's syntax correction and its dependency/contract equivalence.

## Parent-authored concurrent-delivery fixture correction

**Scoped DDD/public-contract PASS; no P1/P2 finding in this fixture change.** Reviewed the actual `process_pressure.rs` diff setting `drain_timeout: Duration::from_secs(2)` only for the sixteen output producers. File SHA-256: `798131c858e38e3f4c22c7581428fe5ea3d1346cf9c6129ce20b3f93e1111137`. No production code, public API, default limit, buffer capacity or acceptance outcome changes.

`ProcessLimits` already exposes the drain deadline as caller-selected policy, and `DrainOutcome::Truncated` explicitly permits deliberate deadline cutoff independently of the child's exit result. The lifecycle adapter enforces that deadline after observing exit. Giving this complete-delivery fixture an explicit larger finite drain budget therefore uses the public contract at the infrastructure composition boundary; it does not bypass the domain or change the adapter's responsibility. The test still requires all sixteen children to exit zero, report EOF and deliver exactly 131072 zero bytes each. It does not accept truncation, skip producers or retry failures. The quiet cancellation case still uses default limits, and `descendant_endpoint_has_bounded_drain_separate_from_exit` retains its explicit 40 ms budget, exit code 23, exact `final` output and Truncated assertion.

The original hosted failure log `/tmp/pty-coverage-macos-failure.log` shows actual Truncated versus expected EOF after the preceding exit-code assertion succeeded. This demonstrates that the old fixture reached a permitted drain outcome which its complete-delivery assertion rejected. It does not by itself prove host scheduling was the sole cause or eliminate a performance regression. This correction separates complete-delivery correctness from the default 250 ms cutoff; it supplies no assurance of complete delivery within the default cutoff and must not be described as repairing a production defect. Independent adversarial diagnosis, affected test reruns and performance evidence remain separate requirements. No builds or tests were launched by this reviewer; the parent's focused integration run was pending at review time.

## Drain-budget candidate rejected; CI closure remains open

The proposed 2-second producer fixture budget above was **not integrated** and is no longer the working source. This reviewer verified `process_pressure.rs` has no diff from HEAD and again passes `ProcessLimits::default()` for all sixteen producers; its restored SHA-256 is `340009323ba15d383c4d49909a5c63bad617e511bc6ef9ed878814f38c5095f8`. The parent identifies the restored revision as `a3e0fee`.

The parent reports that an independent delayed-acceptor diagnostic also produced Truncated under the 2-second candidate despite collecting complete bytes. A longer fixture deadline is consequently not a verified resolution of the hosted failure. The preceding DDD pass established only that an explicit deadline was allowed by the public contract; it did not establish causal correctness or CI closure, and must not be used as acceptance of this rejected candidate. The passing `gate-drain-final` run applies only to that rejected source and does not certify the restored candidate.

**CI-closure P2 remains OPEN pending causal tracing and a supported resolution.** No production defect or production fix is claimed. Preserve the original failure and rejected-candidate evidence; any future change requires affected reruns and independent review against its actual source. This update changes only the review ledger; no builds were launched.
