# Independent root-wiring Clean Code / SOLID review

Review date: 2026-09-08. Reviewer: correctness_review, independently examining
root-authored wiring. Scope: `src/adapters.rs`, `src/runtime.rs`, and application
runtime `owner.rs`, `context.rs`, `projected.rs`, `lifecycle.rs`, and
`session_projection.rs`. This reviewer did not author those production files.
The coordinator's internal organization is excluded from this review and covered
by the separate root-authored coordinator review.

Current result: **no unresolved P1/P2 organization or SOLID blocker in scope**.
This is a source organization assessment, not a full milestone or gate pass.

## Review method and challenges

I read the actual source first, including the final domain-validation call in
`Runtime::spawn`, and then read `projection-wiring-ddd.md`. I independently
challenged responsibility boundaries, ownership duplication, port design,
error propagation, rollback and teardown patterns, and whether the modules are
merely small files around one uncontrolled object. The conclusions below are
based on the current implementation rather than the author's rationale.

| Challenge | Source evidence and assessment |
| --- | --- |
| Does the facade mix infrastructure setup with application policy? | `src/adapters.rs` constructs the concrete store, protector, workers, clock and terminal. `src/runtime.rs` selects this default composition or accepts explicit ports and delegates use cases. Feature switches remain here. The application does not select Ghostty, crypto formats or a filesystem store. This is an appropriate composition root. |
| Has `Runtime::spawn` become a policy/transport implementation? | It now invokes domain `SessionOptions::validate(&RuntimeOptions)` and retains only installed-capability checks, identity/repository reservation, model/process creation and rollback. The pure cross-budget predicates were moved out. The remaining ordered work is one cohesive spawn use case. |
| Is `ProjectionRuntime` a duplicate session aggregate or generic service locator? | Its fixed slots retain the same coordinator Arcs; they do not copy byte positions, residency policy, source references or process state. It exists because a caller-selected repository cannot be the sole teardown owner. The services are explicit typed collaborators, not dynamic lookup by key/type. |
| Is rollback spread across fragile manual counters? | `Admission` owns in-flight spawn accounting. `ProjectionRuntime::Reservation` owns an unpublished inventory slot and releases it on error. Repository rollback remains explicit where the domain distinguishes admission failure from process failure. Replacing these distinct outcomes with a generic transaction framework would obscure the lifecycle. |
| Does `SessionContext` become an infrastructure owner or callback cycle? | It holds domain replay/status, bounded observers, and application process/projection collaborators separately. The process event target is weak. Output acceptance feeds the authoritative queue before raw publication; neither native nor OS implementation enters this context. |
| Are injection interfaces too broad or tied to one adapter? | Raw construction requires only repository/process ports. Projection construction explicitly supplies its additional boundaries. There are no downcasts or concrete-adapter tests in application wiring. The worker ownership and shutdown contracts are documented. |
| Is cleanup failure concealed by a generic success path? | `ProjectionRuntime::close` now reads the coordinator's atomic close outcome; `Runtime::forget` propagates it before removal. The owner no longer treats Closed as proof of successful provider deletion. This preserves the abstraction's result rather than interpreting internal state ad hoc. |

## Responsibility and ownership assessment

`AdmissionGate` is a small synchronization component. It does not implement
native lifecycle, repository policy or cancellation. Its guard makes shutdown's
relationship to accepted spawns explicit without holding a lock across process
creation. Keeping this concrete is preferable to adding an interface around
internal synchronization.

`ProjectionRuntime` owns model admission and teardown inventory, with bounded
preallocation and a concrete reservation state. Its shutdown takes the inventory
outside the mutex, closes models, and only then joins/releases services. The
exceptional finalization path runs after both worker pools join. That ordering
is visible in one place. The separate caller registry continues to serve lookup
and lifetime-checked removal, so repository replacement does not accidentally
change resource ownership.

`SessionContext` is the stable per-lifetime state used by handles and process
events. Its event adapter and replay accounting are related ingress operations;
I do not find a justified P2 in keeping them together at the current size. The
callback path releases the state lock before waking arbitrary observers. If the
future ordered observer stream adds substantial projection/delta machinery,
that should receive a separate responsibility review rather than being added
indiscriminately to this file.

`session_projection.rs` is a narrow public-handle bridge. It exposes typed
observations and ordered controls, documents abandonment semantics, and keeps
coordinator access private. It does not reproduce provider/native operations.
The ordinary session/process handle remains usable independently of projection
failure. This is an appropriate separation of the two concerns.

The facade's thin wrappers are useful library boundaries, not redundant service
layers: they provide default infrastructure composition and a stable public API.
The small `options.rs` re-export does not duplicate domain models or validation.
No strategy/factory/repository abstraction beyond the actual external ports is
needed for these internal components.

## SOLID conclusion and prior findings

Single responsibility is adequate at the module/use-case level. Dependency
inversion is explicit at replaceable boundaries; the root is correctly the place
that knows concrete adapters. The injection contracts define finite worker work,
shutdown ownership and early process events without adapter-specific branches.
Interfaces are segregated enough that raw users need no terminal/provider setup.
Extensions to terminal/provider implementations do not require rewriting the
application use cases.

The other wiring review identified two P2 issues: pure admission policy placed in
application, and cleanup outcome erased by the ownership bridge. I independently
inspected their final changes: domain validation is invoked before allocation,
and the bridge propagates the actual close outcome. Both responsibility defects
are resolved in the reviewed source. No new P1/P2 finding was discovered in this
organization pass.

All seven reviewed production files are below the 350-nonblank-line threshold;
the largest is `context.rs` at 258, followed by `owner.rs` at 186 and
`projected.rs` at 155 in the inspected formatted source. Size alone was not the
basis for acceptance; the ownership and dependency assessment above was.

## Verification boundary

Per the root's explicit coordination request, this final organization pass ran
no tests and no concurrent full gate. It made no production changes. The root
will run the final mechanical gate after the cleanup/domain regressions settle.
Behavioral results, performance records, and coordinator DDD findings remain in
`projection-correctness.md`; the other independent wiring review is in
`projection-wiring-ddd.md`. Ordered observer continuation and all unclosed G2/G3
qualification remain outside this narrow organization acceptance.
