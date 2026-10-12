# V12 public-contract test quality review

Verdict: **PASS for these two test changes**, with no unresolved P1/P2 finding.
This is the independent code-quality review; the separate adversarial review
and required full mechanical/performance gate remain independent requirements.
No build or benchmark was run by this reviewer.

The reviewed diff changes only `tests/projected_runtime.rs` and
`tests/projection_correctness.rs` (75 insertions, 11 deletions). It adds no
production branch, private-state mutation, compiler adjustment or coverage
exclusion. The extracted `projection_services` helper preserves the existing
adapter composition and gives both tests one cohesive composition point.

The real PTY/Ghostty test now calls raw `Session::resize` on a projected session
and requires the exact Unsupported error. It checks the processed cursor,
control generation and lack of failure, then compares the complete projected
view with the independently maintained Ghostty reference. The preceding view
barrier has already consumed the fixed echoed suffix, so the cursor equality
is not based on an arbitrary sleep during active output. The existing following
projected resize still requires both OS and model success and the correctly
resized reference view. These assertions protect a public ordering boundary
rather than merely exercising its error branch. They do not independently
observe whether the rejected operation briefly issued an OS resize; the current
source returns before calling the process adapter.

The repeated `Runtime::with_projection` test exercises the consuming public
application API. It establishes one live original terminal before rejection,
requires exact InvalidConfiguration, then uses the retained session to require
Closed residency and verifies the original native owner count falls to zero.
This tests the consequential teardown contract, not just the enum variant.
Rejected replacement services remain caller-owned; explicit shutdown stops
both retained pools. Existing adapter observation state is used without
altering runtime internals or poisoning locks. The public API and adapter ports
keep the test dependencies appropriate to the application ownership boundary.

Qualification: replacement alive-count zero and an empty operation trace prove
no retained replacement terminal and no recorded operations. They cannot prove
that a temporary terminal was never created then dropped because Factory::create
is not traced. Source inspection confirms the existing early return precedes
ProjectionRuntime::new. No broader dynamic no-construction claim is made.

Saved evidence `work/public-gap-tests-v12/targeted.json` records
`cargo test --test projection_correctness --test projected_runtime --all-features`
exiting zero with all seven tests passing. The author also reported a passing
format check. The complete canonical coverage report has not been rerun for
these changes; no coverage gain or complete acceptance is claimed. Journal
poison/error closures and private DataCounterGuard overflow paths were not
filled with synthetic tests merely to change counters.

Reviewed file SHA-256 values:

- `tests/projected_runtime.rs`: `2e3d7ff5922569600ea3d882269e8f8b403f94ee1ef15426f11763d6a3f3fc90`
- `tests/projection_correctness.rs`: `21479add427ad2a863f651ee60ccea4e64c980c1a23992721f7e4adca67b526d`
