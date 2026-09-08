# ADR 0005: Domain models, ports, and infrastructure adapters

**Status: implementation in progress.** The Rust workspace enforces domain,
application, infrastructure, and facade boundaries through its mechanical gate.
Integrated orchestration remains under implementation and specialist review.
Ghostty is the initial terminal engine and must remain replaceable.

## Decision

Keep session behavior in our domain and application layers. Put Ghostty, Unix
process APIs, readiness systems, files, encryption libraries, clocks, and the
event-stream dependency behind interfaces implemented in infrastructure. Convert
external representations into our types at those boundaries.

Expose one consumer-facing `pty-runtime` package. Use internal domain,
application, and infrastructure crates within this repository so Cargo enforces
dependency direction. The public facade composes defaults and accepts injected
implementations. No sibling package changes are required.

| Layer | Owns | Allowed dependencies |
| --- | --- | --- |
| Domain | Session aggregate, IDs/lifetimes, cursors, terminal views, state transitions, resource policies, typed outcomes and errors | Rust standard library and other domain types |
| Application | Spawn/attach/input/resize/cancel/park/restore use cases; orchestration; ports and repository contracts | Domain and standard library; no native, OS, storage, event-stream, or executor implementation |
| Infrastructure | Port implementations, native bindings, platform I/O, scheduling, storage, encryption, serialization DTOs, external-error mapping | Application ports, domain types, and the external dependencies it adapts |
| Public facade | Runtime construction, default adapters, configuration, public API and examples | Application and infrastructure; returns our public types |

Infrastructure depends on application/domain contracts. Core code must never
import an infrastructure implementation to complete a use case. Keep executor
and synchronization integration in infrastructure; application policies must be
testable without starting an OS reactor or loading Ghostty.

## Domain model and repositories

The session aggregate represents one registered workload lifetime. It owns the
rules for identity, state transitions, replay positions, admission, cancellation
intent, and checkpoint generations. Process exit, output-drain completion,
terminal residency, and observer attachment remain distinct states.

Use our own models for commands, dimensions, byte chunks, exit outcomes, cursor
gaps, terminal modes/styles/views, generated effects, checkpoint descriptors,
restoration progress, and errors. Native handles and descriptors are owned by
adapters, not stored in domain models. Serialized JSON or database records are
boundary DTOs, not the domain's source of behavior.

The application coordinates external work and feeds its translated outcomes
back into domain transitions. For example: admit a resize, request the OS and
terminal operations through ports, then publish the actual outcome. The domain
must not interpret errno values or Ghostty result codes to decide what happened.

Declare injected interfaces in the application layer, using I-prefixed names.
Repository operations specify atomicity, ownership, bounds, and error behavior.

| Port | Domain-facing contract | Initial infrastructure implementation |
| --- | --- | --- |
| `IProcessBackend` / `IProcessSession` | Spawn; byte I/O; resize/control outcomes; exit and drain events | Unix PTY adapter with separate macOS and Linux implementations |
| `ITerminalFactory` / `ITerminal` | Create/feed a model; domain views and effects; resize; checkpoint and restore | `GhosttyTerminalFactory` / `GhosttyTerminal` wrapping pinned libghostty |
| `ISessionRepository` | Atomic identity registration, lookup, versioned state access, explicit finished-session removal | Bounded in-memory registry |
| `ICheckpointStore` | Commit/read/delete opaque encrypted checkpoints under the existing storage contract | Built-in `FileCheckpointStore`, or caller-injected provider |
| `ICheckpointProtector` | Protect/open checkpoint bytes with authenticated metadata and typed failures | Vetted encryption adapter with owner-lifetime keys |
| `IEventPublisher` | Publish our bounded session events with explicit failure/gap outcomes | Optional event-stream adapter |
| `IClock` | Supply monotonic domain time for policy decisions | System clock; deterministic clock for tests |

The session repository does not imply durable process recovery or a database.
Keep application-owned live collaborators separate from persisted DTOs. Resolve
session identity through the repository and retain that context for hot I/O;
avoid a repository lookup for every byte.

Interfaces belong at external boundaries. Use concrete domain types for internal
rules and transformations; do not introduce an interface for every struct or
byte operation.

## Terminal engine boundary

`ITerminal` must not expose `GhosttyTerminal`, native cells, C pointers,
Ghostty enums, allocator handles, or native error codes. The adapter converts
cursor, screen, style, mode, and reply information into our terminal models.
External callbacks produce bounded domain effects; the application serializes
authoritative replies through the process port.

The contract requires correct incremental parsing, partial-sequence continuation,
ordered resize, supported terminal queries, snapshots, and complete restorable
state for projected sessions with default parking. Optional features such as
resident-history compression are expressed as capabilities, with an explicit
unsupported outcome. A replacement engine must meet required behavior or fail
configuration validation. It cannot silently discard state or disable required
parking semantics.

Use engine-neutral restoration progress such as usable state and complete
history. The Ghostty adapter maps its READY/history API into that model. An
engine without incremental restoration can report usable and complete together
after full restoration. Core scheduling must not depend on Ghostty page layouts
or assume every engine supports the same compression operation.

### Checkpoint compatibility

A checkpoint descriptor carries an opaque engine/format compatibility identity,
the session lifetime, processed byte position, and ordered control generation.
Only the terminal adapter interprets its encoded payload. The store sees
encrypted bytes and has no engine-specific logic.

Replacing an engine for future sessions is a normal configuration change.
Changing an existing session's engine or reading another engine's checkpoint
requires an explicit, verified conversion. Otherwise return a typed compatibility
error and preserve the existing session. A native checkpoint is not a universal
format, and truncated byte replay cannot rebuild a complete replacement terminal.
These constraints belong in the interface contract.

## Boundary conversions and errors

| External representation | Converted at | Core receives |
| --- | --- | --- |
| Descriptor events, wait status, termios/ioctl failures | Platform process adapter | Byte chunks, exit/drain outcomes, resize/control results, typed process errors |
| Native terminal state and callbacks | Terminal engine adapter | Terminal view, modes, bounded reply/effect messages, restoration progress |
| Native snapshot bytes | Terminal adapter, then checkpoint protector | Opaque compatible state plus our checkpoint descriptor |
| Filesystem or custom-store responses | Store adapter | Committed checkpoint reference or typed storage failure |
| Event-stream records/cursors | Event publisher adapter | Our session events and publication outcomes; its cursor stays distinct from our replay cursor |

Keep serialization derives and external DTO schemas in infrastructure. Validate
sizes, enum values, encoding compatibility, and required fields before converting
to domain models. Translate expected external failures into our stable error
categories. Preserve context through redacted infrastructure diagnostics without
leaking external types or terminal contents into core APIs.

Conversions need not serialize or copy every byte. Pass bounded owned/borrowed
byte buffers through ports, reuse immutable payloads where ownership permits,
and account for shared pins. Dispatch per chunk or admitted operation. Avoid
per-byte dynamic dispatch or JSON inside the I/O path. Include adapter and
conversion overhead in integrated performance tests.

## Platform implementations

Keep readiness, supervision, allocation, and reclamation mechanics in
platform-specific infrastructure modules implementing the same process/session
contract. macOS uses kqueue and its native measurements; Linux uses epoll and
Linux measurements. Default to one dedicated reader per live PTY, including
quiet sessions, with session admission bounding reader count. Keep shared reader
strategies replaceable behind the same process port. Qualify stack/scratch costs,
interruption, allocation strategies, and fairness per platform and workload.

The domain enforces portable logical limits. Platform RSS, PSS, charged footprint,
page size, and reclaim behavior inform configuration and diagnostics without
becoming interchangeable counters or Apple-specific domain rules.

## Implementation and verification gates

- G1 establishes dependency boundaries, domain models, process/repository ports,
  typed error mapping, and platform adapters before native projection.
- G2 implements the terminal contract with real Ghostty in infrastructure.
  Test core orchestration with a deterministic test adapter and run the contract
  suite against the real engine. Test doubles are not native integration evidence.
- G3 implements store/protector adapters and DTO/compatibility validation, then
  exercises parking races with default and injected stores.
- CI builds/tests domain and application independently and rejects forbidden
  dependency edges using Cargo metadata. Review public/core APIs for external
  types and test conversions, unsupported capabilities, and compatibility errors.

These gates extend [ADR 0004](0004-integration-and-release-qualification.md).
The [mechanical gate](../../scripts/gate.py) now checks Cargo dependency direction,
file sizes, formatting, lint, tests, and documentation. The separate
[experiment runner](../../experiments/README.md) validates measurement fixtures.
