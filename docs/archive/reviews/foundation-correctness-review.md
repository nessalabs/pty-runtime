# Foundation adversarial correctness review

Reviewed 2026-09-08: `crates/domain/src/identity.rs`, `replay.rs`, their tests,
and `scripts/gate.py`. Scope is the initial value/replay foundation, **not G1
completion**. No process runtime, global resource manager, native integration,
parking, or release workload is established by these tests. Full requirements
remain in [the ledger](../verification/requirements.md).

## Findings and disposition

| Severity | Evidence | Finding / disposition |
| --- | --- | --- |
| P2, resolved | `identity.rs`, `SessionId::new` originally retained its supplied `String` | A one-byte ID could retain an arbitrarily large preallocated backing buffer despite bounded metadata policy. Author changed storage to `Box<str>`. Reviewer re-read the implementation and added a 1 MiB-capacity synthetic input regression asserting the accepted allocation's capacity equals its length after conversion back into String. Test passes. This asserts the owned allocation contract, not immediate physical RSS reclamation. |
| P2 verification gap, resolved for foundation | Existing `replay_tests.rs` reference test only read the floor after append | It did not cover alternating pressure eviction, append, arbitrary observer positions and bounded pages against a reference. Reviewer added independent append-only-ledger integration test, retaining the full produced stream and an independently updated earliest cursor. Five capacities × 32 deterministic seeds × 400 mutation steps = 64,000 operations; five cursor positions per step, repeated reads, foreign cursor and zero-page checks. Test passes. |
| P2 verification gap, resolved for foundation | `identity.rs` previously had no value-policy tests | Reviewer added empty/128/129-byte, Unicode byte-length, ASCII/Unicode control, literal equality, lifetime component and redaction checks. Tests pass. |

No remaining correctness defect was found in the reviewed replay/value scope.
That statement does not apply to future orchestration or prove bounded runtime
memory: `ReplayBuffer` deliberately bounds logical bytes only. `VecDeque`
capacity, temporary growth/copies and caller-owned pages require application
reservations before the primitive can satisfy G1 resource contracts. Pressure
eviction can retain allocated capacity; its existing documentation correctly
separates logical eviction from allocator reclamation. Reading does not mutate
an observer cursor; acknowledgement/cancel races need application tests later.

Lifetime values currently accept caller-composed owner/sequence pairs. The
future runtime must issue non-reused pairs with checked exhaustion and atomic
registration. Comparing the two fields is correct, but it cannot establish
unique issuance, PID identity, replay cursor authorization, or process lifetime
ownership by itself.

## Gate review

The current architecture gate checks required workspace package locations,
allowed core dependency names and local dependency paths, then rejects reversed
known workspace edges. It checks all dependency kinds listed in metadata and
therefore should also reject external core dev/build dependencies. Core crates
are tested independently, but this is not a semantic check that application
code avoids `std::fs`, threads or other implementation detail; specialist review
remains necessary. File-size limits are mechanical alarms, not cohesion proof.

Before treating the gate itself as well-tested enforcement, add negative tests
for forbidden normal/dev/build/target/optional dependencies, dependency renaming,
wrong paths, missing/relocated members and file-size overflow. Its initial
`cargo metadata` invocation should use `--locked` consistently with later build
commands so a gate invocation cannot silently resolve a changed dependency graph.
These are gate-hardening follow-ups, not claims that current core code imports a
forbidden dependency. This reviewer did not inject mutations into the shared
workspace or run the complete gate; the author must preserve the final full-gate
output after all specialist changes.

## Executed verification

Command: `cargo test --locked -p pty-runtime-domain`, macOS arm64, 2026-09-08.
Result: eight unit tests and one independent replay model test pass; zero failed;
no Rustdoc examples exist in this slice. Existing five tests were executed before
review additions and also passed. The new tests exercise public replay API from
an integration test; only identity's allocation check uses a colocated private
field, since resource shape is not exposed through the public API.

This is deterministic state coverage, not threaded concurrency, stress, native,
platform, or performance qualification. The overflow unit test covers checked
append failure; actual process/observer lifetimes and global pressure require
later integration evidence. No ADR gate is marked complete by this review.
