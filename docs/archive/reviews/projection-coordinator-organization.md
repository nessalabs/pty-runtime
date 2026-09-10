# Projection coordinator organization and ownership review

Scope: independent review by the root integrator of the coordinator implementation
written by the scheduler specialist. The integrator authored the runtime/facade wiring;
that wiring is excluded here and reviewed independently in projection-wiring-ddd.md.
Reviewed admission, budgets, completion, coordinator, IO, native execution, observation,
state, teardown and worker modules, and their domain policy dependencies.

The coordinator has one exclusive native owner and a separate short admission lock.
Its scheduler and blocking-executor interfaces represent different external execution
boundaries. The state enums describe pending work; native/provider implementations do
not enter the application. The per-responsibility implementation modules share a
single coordinator without duplicating authoritative state or introducing service
lookup. Lease ownership follows retained observations and in-flight provider work.
The runtime's resource inventory retains these same coordinator objects and is not a
second session aggregate. These choices are appropriate for the current scope.

## Resolved finding

P2: `native.rs`, `Event::Checkpoint`, reserved `protected_bytes` while returning a raw
`TerminalCheckpoint`. The protector interface permits a protected size bound smaller
than the plaintext cap. A compressing implementation therefore allowed retained raw
snapshots to consume more memory than their reservation. The parked transfer path
already transfers `IoMemory::plain` correctly. Requested change: charge the native
snapshot's plaintext cap and add a compression-bound regression that proves retained
pins prevent over-admission. Resolved: `native.rs` now reserves `engine.config.checkpoint_bytes` before native
allocation. Independently inspected the change and the regression, then ran
`cargo test -p pty-runtime-application resident_checkpoint_pin_reserves_plaintext_even_when_protection_bound_is_smaller`:
one test passed. With a 1025-byte pool, the first 1024-byte reservation prevents a
second pin despite the protector advertising only one protected byte; dropping the
first pin admits a third request. The protector deliberately errors if invoked, so
the regression also establishes this path remains a raw native snapshot.

## Further review limits

This review does not accept the full ordered state-transfer ADR: the current snapshot
pin has a byte/control boundary but no ordered continuation of output and resize
controls. That functionality and its own bounds/observer tests remain required.
The full mandatory gate passed after review changes on macOS arm64 and Linux
x86_64 with identical source manifests; see [loop 3](../milestones/loop3.md).
No unresolved organization/ownership blocker remains in this coordinator scope.
Root-wiring findings and behavioral/domain review findings are recorded separately.
The outstanding ordered transfer and release requirements still prevent full ADR
acceptance.
