# Independent review of final coverage tests and scanner change

## Scope and verdict

Scoped code-quality review: **PASS, no unresolved P1/P2 finding identified**.
This is not a mechanical, coverage, performance or release acceptance. The
previous failed performance cohort and coverage limitations remain applicable
until their separate gates establish otherwise.

Reviewed the current tracked diff and new Rust test files at base revision
`5c7434ac2c129870dc6b1adbfa9a5d493b444cdd`, with source digest
`07ea3217667be0e547ec5fd403ef8175c234dc98832a3cca6facb3158c29c092`
from `scripts/performance.py::identity`. The reviewer did not author the added
domain/application tests and ran no builds or tests during this review.

The review includes the final additions in domain `public_boundaries.rs`,
projection `transfer.rs` and `admission_rollback.rs`; the prior coverage batch's
runtime, projection, infrastructure, guardian and facade tests; native boundary
shim/coverage-runner changes; test-module registrations; and the three-line
scanner fast path. Findings and prior corrections in
`coverage-closure-quality.md` and `coverage-closure-application-fix.md` remain
part of the record.

## Final added tests

The domain cases drive existing validated values and policy methods. Overflow
checks compare complete before/after policy status, rather than merely accepting
an error; the late parking failure case verifies a closed lifetime stays closed
and cannot commit an old attempt. They add no domain dependencies, test-only
constructor or production visibility. Numeric boundary values express actual
capacity/overflow contracts, rather than introducing a parallel implementation.

The transfer case has two simultaneous observers. It retains and rereads the
old observer's event, proves the later observer cannot cross its initial
boundary, and verifies delivery of new output. Thus refusal is not explained by
global eviction or the absence of an event. The final offset and resource-release
checks give observable behavioral and ownership assertions; internal call order
is not the asserted contract.

The backend-refusal case uses the public Runtime with existing replaceable
process/repository/scheduler ports. The same ID and sole runtime slot are reused
twice, while factory model lifetime, backend entry, missing registry identity and
every projection resource budget are checked after each refusal. This proves
rollback restores usable capacity, rather than simply testing one error return.

Its local `CloseDriver` has one narrow responsibility: make asynchronous close
progress deterministic for the fixture. Wakes are coalesced, weak ownership is
retained, work runs after releasing the driver registry lock, and a sixteen-turn
limit makes a stalled close fail. It does not introduce a new production
scheduler or claim to validate real OS scheduling. Keeping this fixture beside
admission rollback tests is cohesive despite the module's size; no generic
testing framework or shared abstraction is warranted by this one scenario.

Read the supplied focused logs under
`work/coverage-closure/followup-public-tests`: nine domain public-boundary tests,
the backend rollback test, and the transfer observer test passed. These logs are
affected-check evidence, not a substitute for the subsequent frozen full gate.

## Full-diff design and ownership assessment

The remaining test additions preserve layer direction: domain uses std-only
values; application failure fixtures implement existing external ports;
infrastructure tests exercise real filesystem/protection/scheduling adapters;
guardian tests use actual socket pairs or fresh helper processes. The native
fault shim is confined to test compilation and checks conversion behavior without
claiming real Ghostty produced synthetic return values. The native coverage
runner adds existing real contract targets rather than excluding production
paths to improve a percentage.

Reviewed the corrected descriptor precondition (CLOEXEC is cleared before
adoption), scoped child cleanup, bounded channel/process waits, and release of
owned observer/checkpoint results. Prior private counter/poison manufacturing is
absent. Narrow low-level session/callback unit fixtures remain limited evidence
about those component contracts; the public Runtime tests separately cover
ownership and admission. None is represented as native-process integration.

The scanner shortcut reuses its existing state predicate and preserves domain
information ownership. Ground/no-escape input previously exited the search
without mutations; the new early return is equivalent. Stateful continuations
and chunks containing escapes retain their former path. Store validation, plan
identity and commit-generation checks still execute. This review introduces no
new concern beyond the performance scope and continuation-validation duties
already recorded in `coverage-closure-performance.md`.

No source was changed in this review. Acceptance remains conditional on the
independent adversarial review, final mechanical evidence, honest coverage
measurement and the prospectively declared performance gate at the final source.

## MSRV compatibility finding and independent correction review

Correction: the preceding quality pass missed a P2 compatibility defect in the
new `CloseDriver` fixture. Its `&& let Some(work) = ...` chain is unsupported by
the declared Rust 1.85 toolchain. The hosted failure at
`/tmp/pty-coverage-msrv-failure.log` reports E0658 at
`admission_rollback.rs:378`; the library test build exits 101. A stable-toolchain
pass did not establish minimum-supported-version compatibility.

Reviewed the actual replacement at base commit
`328c6967226ae0c084ce8702f13c88a0f5bf5e29`, current source digest
`8f137114df26f12d1d679f735d47c0c83a55c72f74c1f188c65ad6a8806b2018`.
The fixture now continues early when closed or when no wake was pending, then
uses the Rust-1.85-supported `let Some(work) = ... else { continue; }` before
running the work. Short-circuit evaluation is preserved: a closed handle never
clears pending, an open handle clears pending once, and weak upgrade/run occurs
only when that swap observed a pending wake. Work still runs outside the
registry lock, with the same bounded driver turns and finish/idle handling.

The change is local to test orchestration, adds no dependency or production
visibility, and improves control-flow clarity without introducing an abstraction.
The focused Rust 1.85 evidence at
`work/coverage-closure/msrv-fix-focused.log` shows successful compilation and a
passing backend rollback test. **The scoped P2 syntax defect is resolved on
independent re-review.** The complete Rust 1.85 matrix and subsequent full gate
remain separate checks; their outcomes are not inferred from this one test.

No build or test was run by this reviewer. The three-line production scanner
candidate is unchanged by this compatibility fix. Its recorded performance
failure remains applicable; this test-only change supplies no basis for a new
timing cohort or for upgrading readiness.

## Concurrent-output fixture drain deadline review

The hosted macOS failure at base commit
`a3e0fee3c5974e22486673d94cade302ad9b39ee` is retained at
`/tmp/pty-coverage-macos-failure.log`: the concurrent producer test observed
`Truncated` where it asserted `Eof` (`process_pressure.rs:49`). This is distinct
from the resolved MSRV syntax failure.

Reviewed the actual test-only diff at source digest
`0a5cf055b2b18767741e8830da0dc2e337165d74b112b8e3f946f48ee6bea173`.
Only the sixteen finite producers now receive an explicit two-second
`drain_timeout`; the quiet process keeps default limits. Exit code zero, EOF,
exactly 131,072 bytes, and every byte zero are still required for every producer.
No production deadline, output assertion, performance threshold or coverage
denominator changed.

The former fixture combined a complete-delivery assertion with a default
250-millisecond post-exit cutoff. The lifecycle checks that elapsed deadline and
stops the reader; the reader reports `Truncated` when stopped. Therefore process
exit zero alone never guaranteed EOF before this cutoff. A larger explicit,
still-bounded drain allowance is consistent with this test's concurrent-delivery
purpose. The separate descendant-endpoint test retains its explicit 40-millisecond
deadline and `Truncated` assertion, so bounded drain behavior is still tested.

This adjustment does remove the implicit demand that all sixteen output streams
reach EOF within the default cutoff on the current host. It must not be described
as proving complete delivery under default limits or as fixing a runtime defect.
The observed CI outcome is consistent with the deadline path, but this review
does not claim to have measured the hosted reader's scheduling delay. The
independent delayed-callback diagnostic is separate causal evidence.

Scoped quality verdict: no blocking design or contract-weakening finding for
the stated complete-delivery fixture. The supplied
`work/coverage-closure/pressure-drain-fix-focused.log` reports all seven process
contract tests and all four process pressure tests passing. Full frozen/hosted
checks remain separate; a single local pass cannot establish absence of all
scheduling-sensitive failures. No builds were run by this reviewer. The runtime
performance failure remains unchanged and is not waived by this test correction.

### Correction: proposed drain allowance rejected and reverted

The proposed two-second allowance is **not an integrated or verified fix**.
The coordinating task reports that the independent delayed-acceptor diagnostic
still returned `Truncated` with complete bytes at two seconds. That result does
not support closing the hosted failure by increasing this fixture deadline.
The earlier scoped design assessment and local focused passes must not be
interpreted as causal proof or acceptance of the CI repair.

Independent read-only inspection confirms `process_pressure.rs` is back to HEAD
with `ProcessLimits::default()` for the finite producers and no remaining diff.
The source digest is again
`8f137114df26f12d1d679f735d47c0c83a55c72f74c1f188c65ad6a8806b2018`.
The passing `gate-drain-final` run applies to the rejected candidate and cannot
close this issue or be represented as verification of a retained change.

**The P2 hosted concurrent-output CI finding remains open.** Further isolated
callback/reader-outcome investigation is underway; no replacement timeout,
production fix or closure is approved by this review. The separate performance
and coverage blockers also remain unchanged. No builds were run for this
correction.
