# Coding standards and mandatory review gate

These standards apply to every implementation/review loop. Run
`python3 scripts/gate.py` before committing and pushing to main. Preserve its
output with the verification record for the reviewed revision. A passing gate
is necessary but does not replace the specialist reviews or ADR acceptance tests.

## Architecture and domain ownership

- Domain owns validated values, state transitions, replay rules, budgets, and typed
  outcomes. Domain uses only the standard library and forbids unsafe code.
- Application owns use cases and I-prefixed external ports. It depends only on
  domain and standard library; no executor, native, OS, serialization, or storage
  implementation may enter its public or private logic.
- Infrastructure implements those ports and converts external DTOs/errors into
  domain values. Ghostty, process APIs, filesystem/encryption, event-stream, and
  scheduler dependencies stay here. The public facade composes/injects adapters.
- Use interfaces at replaceable external boundaries, concrete types internally.
  No service locator, generic repository for unrelated aggregates, or mirrored
  DTO/domain boilerplate without a demonstrated contract need.
- Never expose native pointers, errno, engine enums, or serialization records in
  core types. Avoid per-byte dispatch and unnecessary payload copies.

## Organization and implementation

- Organize by responsibility with narrow entry modules. Separate process spawn,
  I/O, supervision, terminal ownership, storage, and boundary conversion concerns.
- A production Rust, C, or header source file over 350 nonblank lines requires a split before
  this gate passes. Native build helpers and test sources are included in the same size inventory.
  Tests and fixtures belong in focused modules and also remain
  reviewable. The threshold is an alarm, not permission for incoherent small files.
- Public APIs have Rustdoc covering ownership, bounds, cancellation and failures.
  Expected failures use typed results; no panics/unwrap/expect in production paths.
- Unsafe code belongs in infrastructure, with a local SAFETY explanation of
  ownership, lifetime, aliasing and threading obligations. Safe wrappers enforce
  prerequisites. Callbacks cannot unwind across FFI or reenter the native owner.
- Dedicated readers are the initial default. Count stacks/scratch separately
  from control metadata; bound all session/observer/work admission before allocation.
- Keep locks short; never hold registry/domain locks across blocking OS/storage
  calls. A blocked input or projection operation cannot prevent cancellation/reaping.
- Redact commands, environment, bytes and snapshots in Debug/errors. Clear owned
  transient input; never use real credentials in tests.

## Verification and adversarial review

Each review loop requires three independent specialist reviews:

1. DDD/dependency direction and domain-to-infrastructure conversion contracts.
2. Strict independent Clean Code/SOLID review: organization, file size, cohesion,
   ownership, dependency inversion and appropriate design patterns. This reviewer
   must challenge the implementation rather than accept its author's rationale.
3. Behavioral correctness: adversarial concurrency, resource bounds, cleanup,
   failure injection, and whether tests prove the actual ADR requirement.

Every acceptance loop also requires scoped performance tests with recorded workload,
source identity, platform and raw results. These do not substitute for the full
release performance/repetition/soak requirements. P1/P2 findings in the accepted
scope block acceptance until independently re-reviewed as resolved.

Record findings with file/line evidence and severity, changes made, and unresolved
items by updating `docs/verification.md` (plain English) and keeping specialist
notes out of the tree unless the user asks for them. Re-review fixes; never mark
an issue resolved only because the author says it is. No unresolved
correctness/architecture blocker may be called a passed milestone. Run the gate
again after review changes.

Use meaningful deterministic tests, real Unix child fixtures and real Ghostty
contract tests. Mocks verify orchestration only. Keep a short plain-English status
in `docs/verification.md` (what works / what is still open). Do not check gate/load
log dumps into git. Performance repeats, lifecycle/reconnect counts and the
12-hour soak remain full release requirements. Do not replace them with smaller
green tests.

Review existing tests against intended behaviors before adding missing coverage.
An independent test reviewer must add missing acceptance tests, and those tests
must pass before readiness is claimed. For newly discovered behavior defects,
retain the failing test result before changing production code, then the passing
result after the fix (behavioral TDD). A flawed fixture assertion is not evidence
of a production defect; explain corrections and preserve the original result.

The user requires 100% code coverage as an additional readiness target. Measure
and report line, function, and region coverage, with branch coverage where the
toolchain supports it. Inventory production Rust, the guardian helper, native
bridge code, and platform-specific paths; distinguish unmeasured code from
uncovered code. Do not exclude production error paths or change behavior solely
to reach the percentage. Report measurement scope and unsupported instrumentation
explicitly, and retain unmet coverage as a readiness gap. Code coverage does not
replace behavior assertions, independent review, or the full ADR workloads.
Run `python3 scripts/coverage.py --output work/coverage/<unique-run-name>` for the
Rust workspace feature matrix and separately instrumented helper. This additional
readiness gate requires both 100% and zero uncovered lines/functions/regions;
its component scope does not discharge the separate native/platform inventory.

The mechanical gate checks dependency edges, file size, formatting, Clippy,
workspace tests, documentation and experiment-validator tests. Extend it with
integration/feature checks as those paths arrive. CI must run the same command.
