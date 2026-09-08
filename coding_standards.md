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
2. Organization, file size, cohesion, ownership and appropriate design patterns.
3. Behavioral correctness: adversarial concurrency, resource bounds, cleanup,
   failure injection, and whether tests prove the actual ADR requirement.

Record findings with file/line evidence and severity, changes made, and unresolved
items under `docs/reviews`. Re-review fixes; never mark an issue resolved only
because the author says it is. No unresolved correctness/architecture blocker may
be called a passed milestone. Run the gate again after review changes.

Use meaningful deterministic tests, real Unix child fixtures and real Ghostty
contract tests. Mocks verify orchestration only. Every ADR receives a proof record
under `docs/verification`: requirement, implementation location, command, platform,
source revision, raw evidence, result and remaining gaps. Record not-run explicitly.
Performance repeats, lifecycle/reconnect counts and the 12-hour soak remain full
release requirements. Do not replace them with smaller green tests.

The mechanical gate checks dependency edges, file size, formatting, Clippy,
workspace tests, documentation and experiment-validator tests. Extend it with
integration/feature checks as those paths arrive. CI must run the same command.
