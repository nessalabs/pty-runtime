# PTY runtime: implementation review

**Status: proposed for review.** This package contains design documents and
experimental evidence, plus a runnable experiment harness. The Rust session
runtime has not been implemented. Review the
scope and decisions below before starting the implementation milestones.

## What we are building

A reusable, headless Rust library that owns real PTY processes, accepts byte
input, retains bounded output for reconnecting clients, and optionally maintains
terminal state through the actual libghostty library. The embedding backend owns
the runtime for as long as sessions must survive client detachment.

The host supplies authorization and transport. The library includes a default
disk checkpoint store; callers can replace it through the storage interface.
Rendering, a standalone daemon, and recovery of running processes after owner
restart are outside this package. No changes to sibling packages are part of
this implementation. Target platforms are macOS and Linux on arm64 and x86_64;
executed coverage is recorded separately from those targets.

## Proposed decisions

- Support hundreds of simultaneously active sessions as well as idle and mixed
  populations. Use dedicated readers as the initial default, accepting bounded
  per-session thread memory for lower measured under-load latency. Keep shared
  readers as comparators and qualify the integrated path before release.
- Keep domain models and application rules independent of external libraries.
  Implement terminal, process, repository, storage, and event ports in
  infrastructure. Ghostty is the initial replaceable terminal adapter.
- Separate process ownership, observer attachment, and terminal residency.
  Detachment never kills or relaunches the process; parked terminals retain PTY
  readiness, cancellation, and exit supervision.
- Choose raw bytes or Ghostty projection at session creation. Give replay,
  lossless parser staging, native terminal state, and pending operations finite
  per-session and global budgets.
- Automatically park eligible projected terminals after 60 seconds without
  output or another model mutation. Use the built-in disk store by default,
  with runtime-managed encryption and bounded storage; allow an injected store.
  Preserve the processed byte cursor, ordered controls, and unfinished parser
  state. Qualify storage races as part of the initial release.
- Pursue at most 4 KiB of idle control metadata per session. Account separately
  for dedicated reader stacks/scratch, replay, native state, kernel PTYs, children,
  and allocator overhead. Use
  reclaimable pools where ordinary deallocation retains substantial memory.

## Read in this order

| Document | Review focus |
| --- | --- |
| [ADR 0001: Architecture and implementation plan](adr/0001-pty-runtime.md) | Scope, API ownership, lifecycle, dependency pins, and delivery order |
| [ADR 0002: Performance, memory budgets, and stability](adr/0002-performance-and-stability.md) | Finite resource contracts, proposed latency targets, and release workloads |
| [ADR 0003: Parking and terminal state transfer](adr/0003-session-parking-and-state-transfer.md) | Checkpoints, restoration, observer synchronization, and deferred reader handoff |
| [ADR 0004: Integration and release qualification](adr/0004-integration-and-release-qualification.md) | Concrete concurrency, cancellation, storage-race, and platform pass criteria |
| [ADR 0005: Domain boundaries and adapters](adr/0005-domain-boundaries-and-adapters.md) | Dependency direction, replaceable engines, domain models, repositories, and boundary conversions |

The milestones are a working process/byte owner, real Rust/Ghostty integration,
default parking with replaceable storage, then adapters and release qualification. Each milestone has an
observable gate in ADR 0004. Automatic reader switching is a separate extension.

## Evidence behind the decisions

| Experiment | Measured result | Limit |
| --- | --- | --- |
| [0001: PTY reader speed and memory](experiments/0001-pty-speed-and-memory.md) | At 128 PTYs, shared I/O used 96 KiB of added charged footprint versus 2,368 KiB for dedicated readers; dedicated readers were about 4.5% faster in this fixture | Aggregate transport from one serial blocking feeder on Apple Silicon macOS; no independent child producers, native parsing, or complete runtime |
| [0002: Native compression, parking, and restoration](experiments/0002-native-terminal-parking.md) | 128 filled models used about 817 MiB charged footprint; compression reduced this to 59–338 MiB. Parking with an experimental allocator left about 3.8 MiB in the process plus 96.6 MiB of snapshot files | Synthetic native C fixtures; includes about 1.7 MiB process baseline and excludes PTYs, children, replay, clients, encryption, and Linux |
| [0003: Concurrent workloads on macOS and Linux](experiments/0003-cross-platform-concurrent-workloads.md) | 245 fixture runs passed per host, including 128 independent active producers and native restoration; worker and allocator results differ by platform | Standalone fixtures on macOS arm64 and Linux x86_64; no integrated runtime or soak |

These experiments support the architecture; they do not establish integrated
capacity or release readiness. Numeric targets remain proposals until the Rust
implementation passes the recorded workloads. Keep new integration results in
`docs/experiments` and record accepted design changes in `docs/adr`.

Use the [experiment runner](../experiments/README.md) for reproducible macOS/Linux
measurements, data validation, and CI regression comparisons on matching hardware.
