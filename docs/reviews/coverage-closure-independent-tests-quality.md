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
