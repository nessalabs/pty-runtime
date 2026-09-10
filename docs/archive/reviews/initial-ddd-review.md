# Initial domain and dependency review

Date: 2026-09-08. Scope: ADRs 0001–0005, before production implementation.
Reviewer role: adversarial review of domain ownership, dependency direction, and
design patterns. This is a requirements review, not evidence that runtime code
passes these requirements. At inspection, the repository contained ADRs and
experiment tooling; no production Cargo workspace was available to review.

## Decision

The proposed ports-and-adapters structure is suitable, provided the implementation
keeps behavioral ownership in the domain and application rather than merely
moving a monolithic runtime into an infrastructure crate. Dedicated readers are
the accepted initial strategy for live and quiet sessions. Reader handoff is an
optional future extension; default terminal parking is a required initial feature.

The requirements below are review blockers when their owning gate is claimed
complete. None is established by the existing transport/native fixtures alone.

## Dependency and ownership requirements

| ID | Requirement | ADR / gate | Required proof |
| --- | --- | --- | --- |
| D01 | Domain depends only on the standard library and domain types; application depends only on domain and the standard library. OS, native, crypto, serialization, event-stream, and executor dependencies remain in infrastructure. | 0005 / G1 | Inspect actual Cargo metadata for normal, build, target-specific, and feature-selected dependency paths; independently build/test both core crates. Check all declared supported feature combinations, not only the default graph. |
| D02 | Native handles, descriptors, OS statuses, allocator handles, external errors, and third-party event cursors do not escape through public/core signatures or type aliases. | 0001, 0005 / G1–G2 | Review public exports, generic bounds, associated types, error sources, and Rustdoc. Exercise adapters' typed conversion failures. |
| D03 | The session aggregate enforces identity/lifetime, legal transitions, cursor validity, admission, cancellation intent, and checkpoint generations. Infrastructure reports facts and executes effects. | 0004, 0005 / G1–G3 | Deterministic core tests exercise transitions without loading Ghostty, opening PTYs, or starting workers. Trace production calls to show the same transition methods are used. |
| D04 | Process state, drain completion, projection residency/availability, and observer state are independent. A single `Running/Finished` enum cannot represent all four. | 0001, 0004 / G1–G3 | Test child exit with inherited descriptor, detached running child, failed projection with live child, and cancellation wait timeout. |
| D05 | Application-owned process/terminal collaborators are separate from stored domain state and boundary DTOs. A domain mutex containing an OS handle is still a dependency breach even if its type is hidden. | 0005 / G1–G3 | Inspect aggregate/repository fields and constructor dependencies; map every collaborator's owner and destruction path. |
| D06 | Runtime ownership outlives session handles and attachments. Explicit completed-session removal permits ID reuse only with a fresh lifetime. | 0001, 0002 / G1 | Drop all handles, reconnect, and confirm same workload; test old cursors and stale completions after remove/re-register. |
| D07 | Every external boundary exposes an I-prefixed application port with explicit ownership, capacity, cancellation, and typed error semantics. Concrete internal rules need no gratuitous trait. | 0005 / all | Review contracts against actual callers and at least one deterministic fake where appropriate; reject interfaces that simply expose external APIs unchanged. |

## Adversarial challenges by boundary

### Session repository and admission

Use the repository as the atomic identity and version boundary, not as a service
locator or a byte-path database. Reserve an ID before executing an external spawn
and define rollback for failed spawn without allowing a second concurrent launch.
A capacity check followed by a separate insertion is insufficient.

Challenge these failure paths under ADRs 0001, 0002, 0004, and 0005:

- Two calls register the same ID while spawn is blocked. Exactly one may execute.
- Shutdown starts after reservation but before the child becomes owned. Either
  admission fails without a child or the runtime owns and cleans up that child.
- An operation completes after explicit removal and re-registration of its ID.
  Its lifetime/version must reject the completion; ID equality is insufficient.
- Multiple sessions simultaneously reserve a shared byte budget. Per-session
  caps cannot substitute for a global atomic reservation.
- A caller supplies oversized input. Reject before copying or allocating its
  owned buffer; count queued and in-flight operations, not only queue entries.

Review whether reservation ownership is represented by a token/guard with one
release path. A reusable resource-reservation type is justified; a generic
repository framework hiding the actual atomicity contract is not.

### Process adapter and supervision

One dedicated reader per live PTY is a strategy inside the process adapter.
It does not justify one permanent reaper, timer, or control thread per session.
The application should request a termination outcome rather than understand
`errno`, signal numbers, `epoll`, or `kqueue` (ADRs 0001, 0002, 0005).

Challenge a quiet blocked reader during shutdown, a full input queue during
cancel, and process exit racing with resize/signalling. Require a documented
lock/ownership order that prevents signalling a reused PID. Dropping a caller's
wait must not revoke an admitted cancellation. Exit status comes from actual
supervision; a timeout or a reader stopping cannot manufacture it. Catching an
I/O error and setting the session to successful completion is a correctness bug.

All admitted user bytes and terminal-generated replies need one ordered writer.
An extra reply path directly invoking an OS write from a native callback violates
ordering, reentrancy, and the application boundary simultaneously.

### Terminal adapter

`ITerminal` owns an engine-neutral behavioral contract, not the shape of the
Ghostty C API. Infrastructure converts snapshots, modes, styles, cursor data,
bounded effects, native failures, and restoration progress (ADRs 0001, 0005).

Required behavior includes incremental partial-sequence parsing, ordered resize,
queries, and complete checkpoint state. Optional compression has an explicit
capability/outcome; required checkpoint behavior cannot silently become optional
when a replacement engine is selected. Validate capability compatibility before
admitting projected sessions.

Challenge callback lifetime, panic/unwind boundaries, and concurrent destruction
of a native handle. Every native mutation, encoding call, and restore step has
one serialization owner; native callbacks produce bounded effects without
reentering the terminal or waiting on a slow sink. A fake engine proves core
orchestration only. The real pinned engine must run the contract suite.

### Parking transaction and storage

Parking is an application use case with domain generation checks. Storage is
an opaque-byte adapter. Do not put parking eligibility, session state, Ghostty
compatibility interpretation, or encryption key ownership into the file store
(ADRs 0003–0005).

The transaction sequence must make these distinctions visible:

1. Reserve bounded encoding/commit resources and record lifetime, processed
   byte position, control generation, and attempt generation.
2. Encode under exclusive native ownership into bounded immutable staging.
3. Protect and commit outside that ownership interval. The provider receives
   authenticated encrypted bytes, never plaintext or a native encoder callback.
4. Atomically revalidate the attempt and absence of pending model activity before
   releasing the resident model. Clean up stale committed data.

Challenge output arriving at each step, re-registration during delayed commit,
encryption failure, a store that cancels after writing, and failed stale-data
deletion. Cleanup/retry metadata and abandoned bytes need finite budgets too.
Do not free the only correct resident state to satisfy a memory target.

A committed checkpoint must remain readable while it is the session's only
copy or an active restore source. Injected stores cannot silently evict it.
Checkpoint descriptors authenticate identity and ordering metadata; validation
must reject mismatched session lifetime, byte/control position, engine format,
truncation, and size violations. An incompatible checkpoint must not reset or
relaunch the session.

READY and complete history are separate domain outcomes. Output resumes exactly
once after usable state exists, callbacks/limits are rebound, and parser
continuation tracking is re-enabled. Failed restoration preserves raw I/O and
process ownership; the bounded lossless parser backlog eventually backpressures
that session rather than silently losing bytes.

### Replay, observers, and event publication

PTY byte cursors, terminal-processed positions, ordered controls, and event-store
cursors represent different things (ADRs 0001, 0003, 0005). Prefer separate value
types and checked construction rather than interchangeable integer aliases.
A resize consumes no PTY byte, so a byte offset alone cannot order synchronization.

Replay may evict and report an exact gap; authoritative parser staging may not.
Snapshot-to-live transfer must pin bounded state and retain ordered intervening
events, with explicit resynchronization after loss. Observer detachment releases
its resources without changing process state or the parser's progress.

The event-stream adapter is an optional anti-corruption layer: sink failure
cannot block PTY draining or silently change replay semantics. A bounded queue
that simply discards records without a domain-visible publication/gap outcome
does not satisfy the contract. Retry count, pending publications, and shared
payload pins are all resources to account for.

## Recurring review gate

Run this review for every implementation/review loop, alongside the repository's
coding standards gate. Record findings against the reviewed commit and rerun
affected checks after fixes. A green static dependency check is necessary but
cannot prove behavioral placement or correct concurrency.

| Check | Reviewer action | ADRs |
| --- | --- | --- |
| Dependency direction | Inspect Cargo graph and changed imports; reject external types or orchestration implementation in core. | 0005 |
| Domain authority | Trace a changed use case from facade to domain decision, port effect, and translated completion. Identify the owner of each invariant. | 0001, 0004, 0005 |
| Pattern fit | Justify new traits, repositories, factories, strategy objects, or state types through an actual boundary/invariant. Reject pass-through abstractions and service locators. | 0005 |
| Ownership and lifecycle | Identify handle lifetime, cancellation admission, cleanup owner, and stale-generation rejection. Check failure paths, not only success. | 0001, 0003, 0004 |
| Atomicity and lock scope | Identify linearization points; ensure global locks never cover I/O, parsing, encoding, or publication. Inject barriers around asynchronous completions. | 0002–0005 |
| Capacity | Account for queued, active, temporary, pinned, and retry allocations at both session and runtime levels. Prove reserve-before-copy behavior. | 0002, 0003 |
| Semantic conversion | Check invalid sizes, foreign/future cursors, unknown external enums, unsupported capabilities, and compatibility errors. | 0001, 0005 |
| Privacy | Inspect Debug/errors/metrics and malformed-input failures for payload or environment disclosure. | 0001, 0002 |
| Proof scope | Distinguish deterministic core tests, native integration, executed OS tests, stress, performance, and soak. Preserve failures/timeouts and exact commands. | 0001, 0002, 0004 |

For each owning G1–G4 report, link implementation and executed tests to the above
requirements, and label missing evidence explicitly. Do not turn the ADRs into
accepted implementation claims until their behavioral and release gates have
actually run. Initial platform support can be narrowed only as explicitly
permitted by ADR 0004, with unqualified targets still identified.

## Foundation review loop

Reviewed on 2026-09-08: root and three internal Cargo manifests, their four
`src/lib.rs` stubs, `coding_standards.md`, `AGENTS.md`, and `scripts/gate.py`.
This review precedes the first implementation push. The core stubs correctly
forbid unsafe code, and their current declared dependency direction is correct.
No behavioral implementation is present to approve.

### Findings

**F-D01 — P1, architecture gate fails open when protected packages disappear or
are renamed.** At `scripts/gate.py:19–25`, restrictions apply only if a discovered
package happens to have a protected name. Missing domain/application packages
are never rejected. A renamed domain crate can acquire forbidden dependencies
while the remaining formatting/build checks stay green. Require the exact
expected workspace manifest paths and package names to be present, then apply
the allowlist. Protecting a layer's identity is necessary for claiming Cargo
enforces the boundary. Status: open at review.

**F-D02 — P2, permitted dependency identity is checked by name alone.** At
`scripts/gate.py:23`, an application dependency named `pty-runtime-domain` is
accepted regardless of its path/source. A different package carrying that name
can supply externally coupled types while the actual local domain remains
clean. Require the dependency to resolve to the intended local domain package
ID/manifest, including aliased dependency forms. This matters once dependencies
are changed; current manifests point to the correct package. Status: open at
review.

**F-D03 — P2, independent core build/test proof is absent.** Commands at
`scripts/gate.py:35–40` all select the workspace. ADR 0005 requires domain and
application to build/test independently; workspace feature unification can mask
a missing feature/dependency assumption. Add separate `cargo test --locked -p`
runs for each core package, with their relevant feature modes. No current feature
causes a failure, so this is a missing gate rather than a demonstrated current
compile bug. Status: open at review.

The declaration-level `cargo metadata --no-deps` check does inspect optional,
target-specific, dev, and build dependency declarations on the protected
packages: those are not omitted simply because the dependency is inactive on
the current host. Do not claim a nonexistent target-dependency bypass. What it
does not prove is semantic placement: `std::fs`, `std::process`, generated code,
or a core build script can implement infrastructure without adding a dependency.
Keep that a mandatory specialist source review, or explicitly forbid core build
scripts until justified. A names/import grep cannot prove domain purity.

### Executed gate challenge

A Python invocation imported the actual `scripts/gate.py` and patched only
`subprocess.check_output` with controlled metadata. It called the real
`architecture()` function for each case without editing production files:

| Injected metadata | Observed result |
| --- | --- |
| `packages: []` | Accepted |
| Package `runtime-domain` declaring `libc` | Accepted |
| Application dependency named `pty-runtime-domain`, path `/some/other/domain` | Accepted |
| Package `pty-runtime-domain` declaring `libc` | Rejected with forbidden dependency error |

These are checker unit challenges, not claims that Cargo emits invalid metadata
or that the current manifests are wrong. Add permanent negative tests for valid
Cargo-shaped metadata, plus a positive check against this workspace, when fixing
the gate. Re-review those fixes before closing the findings.

### Minimal first domain contracts

Implement only the primitives needed to express G1 ownership first, keeping
OS spawning, synchronization, and random identity generation outside the domain:

- `SessionId`: a validated bounded caller identifier with private representation
  and redacted Debug. The exact accepted syntax and byte limit are documented.
- `SessionLifetime`: an opaque unique runtime registration identity distinct
  from `SessionId`. The application/runtime allocates fresh identities, including
  after removal/re-registration. Checked generation exhaustion never wraps into
  a valid old lifetime.
- `ReplayCursor { lifetime, offset }`: private validated fields; foreign lifetime
  and future offsets are distinct typed errors. A cursor below the retention
  floor returns an exact half-open gap followed by the retained suffix. Checked
  offset arithmetic must not wrap.
- `ReplayWindow`: owns floor/end and bounded retained bytes, with append/read
  transitions that preserve `[floor, end)` accounting for arbitrary chunks,
  including one chunk larger than capacity. A cancelled read does not advance
  an attachment cursor. Avoid allocating maximum replay capacity at spawn.
- `ResourceLimits` and `Reservation`: represent finite session/global count and
  byte admission. Domain checked arithmetic decides whether reservation fits;
  the application/infrastructure synchronization owner makes check-and-reserve
  atomic. The reservation has one release/transfer path across failure and
  cancellation. Do not claim a plain mutable counter proves concurrent bounds.
- Separate `ProcessState`, `DrainState`, and `CancellationState` transitions.
  Admission of cancellation is durable relative to caller wait lifetime; only
  translated OS supervision can record actual exit. These can remain focused
  concrete state types rather than a generic state-machine framework.

The first `ISessionRepository` contract needs atomic reserve/register, lookup,
version-checked mutation, and explicit finished-only removal. Define what occurs
between a reservation and process spawn, and which owner rolls back each failed
phase. Do not expose a mutable registry map that lets callers bypass transitions.
The full model also needs distinct control positions and checkpoint generations
when G2/G3 arrive; do not preemptively conflate those with replay offsets.

First meaningful tests should cover concurrent duplicate registration, admission
rollback, ID reuse rejecting old cursors/completions, replay oversize-chunk gaps,
offset exhaustion, and cancellation-wait abandonment. Pure domain tests plus
deterministic application-port tests establish these contracts; real PTY tests
are still required for the owning G1 milestone.

### Foundation fixes re-reviewed

The updated checker now requires all four exact manifest paths, package names,
and workspace membership; checks the local source of permitted workspace
dependencies; rejects reversed edges; and independently tests domain/application.
F-D01 and F-D02 are resolved by inspection and the executed negative suite below.
F-D03 is resolved for the current featureless core crates by inspecting the
independent test commands; feature-specific independent coverage must evolve
when core features are introduced.

Added `scripts/tests/test_gate.py` using genuine current Cargo metadata as its
baseline and isolated mutations of that data. `python3 -m unittest discover -s
scripts/tests -v` passed all 11 tests. Cases include each missing package,
relocation, renamed core, missing membership, forbidden optional dependencies of
normal/dev/build kinds on different targets, wrong local source, registry source
impersonation, aliases, and a reversed infrastructure-to-facade edge. Valid
workspace and valid aliases pass. Include this command in the mandatory gate.

Reviewed new `identity.rs`, `replay.rs`, and `replay_tests.rs`. Their dependency
placement is appropriate. Replay enforces lifetime and future-offset checks,
exact gaps, a logical retention ceiling, checked end arithmetic, and reads that
do not mutate observer state. Domain test execution passed all five current
tests using `cargo test --locked -p pty-runtime-domain --no-default-features`.

**F-D04 — P2, excessive ID backing capacity, resolved.** The initial
`SessionId(String)` implementation validated string length while retaining
arbitrary caller-supplied capacity, undermining bounded metadata ownership. After
reporting this, reinspection confirmed `SessionId(Box<str>)` and
`into_boxed_str()` in `identity.rs:5,13`. This removes excess logical backing
capacity on adoption; allocator-retained physical memory remains a separate
measurement. No code change by this reviewer was required.

Replay reads now use `VecDeque::range(start..)` explicitly. This is a clear
expression of the retained range; no benchmark was run to establish a speed
improvement over the previous iterator form, whose standard-library iterator
specializations may already optimize skipping.

Remaining G1 obligations are intentionally not treated as foundation blockers:
`SessionLifetime::new` relies on application-issued unique pairs; no registry
yet proves that uniqueness. `ReplayBuffer` explicitly delegates global byte and
page-copy admission to the application; no reservation implementation yet proves
those bounds. Public cursor construction is acceptable because consumption
validates it, but callers cannot obtain authority simply by constructing a
cursor. Future attachment admission must separately enforce handle ownership.
This foundation review does not approve an implemented G1 runtime.
