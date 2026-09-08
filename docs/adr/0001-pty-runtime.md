# ADR 0001: PTY runtime architecture and implementation plan

**Status: implementation in progress; release requirements unqualified.** Raw
sessions and concrete adapters exist. Native projection/parking orchestration
and complete milestone proof remain pending in the [ledger](../verification/requirements.md).

Design a reusable Rust package named `pty-runtime` under NessaLabs, with owned
PTY sessions, bounded byte replay, and headless libghostty terminal state.

Performance and stability are release requirements. Use the resource contracts,
qualification workload, latency targets, and failure tests in
[Performance and stability](0002-performance-and-stability.md) as design constraints
throughout implementation.

The first OS-level measurements and their limits are recorded in
[PTY speed and memory experiments](../experiments/0001-pty-speed-and-memory.md). Design for a
future owner with hundreds of simultaneously active sessions, as well as idle
and mixed populations. Keep idle bookkeeping small, bound active work and
latency, share workers and scratch buffers where measurements support it, and
allocate history and terminal state under explicit budgets.

[Native compression and parking measurements](../experiments/0002-native-terminal-parking.md)
now establish the much larger cost of filled terminal models and the importance
of reclaimable allocations. They exercise the real native C API in scratch
fixtures; the Rust package and integrated session runtime remain planned work.

## Decision and rationale

Build a reusable headless Rust session owner with explicit process lifecycle,
bounded byte replay, and separately budgeted terminal state. Ghostty is the first
infrastructure engine behind the terminal port. Keep domain/application logic
independent of it under [ADR 0005](0005-domain-boundaries-and-adapters.md).

Use one dedicated reader per live PTY as the initial default on macOS and Linux,
including quiet sessions. Bound reader admission by the configured live-session
limit; reader count and stack/scratch costs are explicit per-session resources.
Keep the reader strategy behind the platform process interface.

The [concurrent experiment](../experiments/0003-cross-platform-concurrent-workloads.md)
measured 128 active Linux producers: dedicated readers delivered 194.7 MiB/s with
12.94 ms probe p99 and 1.77 MiB added RSS; four shared workers delivered
198.9 MiB/s with 45.32 ms probe p99 and 0.375 MiB added RSS. We accept about
1.4 MiB extra owner RSS for the lower measured latency and similar throughput.
On macOS, dedicated readers also lowered latency but sacrificed throughput and
used more CPU than one shared worker. This is an explicit latency preference,
not a claim of universal performance superiority.

This initial reader decision was accepted on 2026-09-08. The production runtime
remains unimplemented. Qualify integrated native parsing, control responsiveness,
idle costs, and larger populations before release. Keep shared strategies in
the experiment matrix; automatic handoff remains a later extension under
[ADR 0003](0003-session-parking-and-state-transfer.md).

Manage terminal storage, reader placement, and observer buffers independently.
Cold sessions retain read readiness and process ownership while spare memory
is reclaimed. Complete binary checkpoints enable automatic terminal parking by
default, using the built-in disk store or a caller-supplied replacement;
a complete server, transport protocol, and visual interface remain separate concerns.

## 1. Outcome and package boundary

Deliver a reusable Rust library that runs an explicitly supplied executable in a
real PTY, supports asynchronous byte input and output, and keeps the process
alive while clients detach and reconnect. Expose terminal emulation through a
separate headless API backed by the actual libghostty library.
The default engine belongs in infrastructure and can be replaced through the
terminal factory/engine interfaces without changing domain models or use cases.

Target macOS and Linux on arm64 and x86_64.
Use the available Apple Silicon macOS machine for initial validation. Linux
execution must be reported separately from cross-compilation or configured CI.

Deliverables:

- Typed library source and Cargo lockfile.
- Pinned, reproducible libghostty build integration with native prerequisites.
- A built-in disk checkpoint store and an injectable storage interface, with
  runtime-managed encryption, bounds, parking, restoration, and cleanup.
- An interactive terminal example and an `event-stream` example.
- Rust fixture tests and colocated unit tests.
- README, API Rustdoc, lifecycle/resource documentation, third-party notices,
  and a workspace-sdk integration sketch.

The package does not provide a GUI, WebSocket server, cloud service, credential
store, or language bindings. Workspace-sdk source is integration context;
changes to that package are a subsequent integration task.

### Backend ownership model

The runtime is the headless session owner: it manages child processes, their
PTYs, retained output, and terminal state while clients attach and detach. Keep
it in a long-lived backend process independent of any client connection.

The embedding application supplies that owner process and any local IPC or
remote transport. A standalone daemon, socket protocol, and service installation
are separate packaging decisions. Hosting the library inside a short-lived
client cannot preserve sessions when that client exits. Client reconnection
requires the same surviving owner; restarting the owner starts a new lifetime.

Terminal state remains data exposed to callers. Rendering, window/pane layouts,
and visual interfaces are outside the package boundary.

## 2. Verified dependency direction

### Ghostty

The inspected official source revision is
`82232ecde55405559dec29c5466cb9e39938cb41`. Its build manifest requires Zig
`0.16.0`, and its source is MIT-licensed.

Use its headless `libghostty-vt` C API. The current headers expose terminal
allocation, incremental VT writes, resize, cursor and mode queries, and screen
formatting, incremental history compression, and full binary snapshot/restore.
The snapshot decoder can restore usable state before older history. Synchronous effect callbacks can supply terminal-generated replies
to the PTY. Clipboard and desktop effects are opt-in upstream and will remain
disabled in this package.

This API maintains terminal state. Rust's Unix layer must separately allocate
the PTY, establish a controlling terminal, spawn the process, and manage its
lifecycle. The upstream C API is explicitly evolving, so the wrapper must be
bound to the exact source revision and generated against matching headers.

Upstream describes the VT library as usable on macOS, Linux, Windows, and
WebAssembly. That does not extend this package's Unix process support to Windows
or WebAssembly.

Sources:

- [Official embedding overview](https://github.com/ghostty-org/ghostty#cross-platform-libghostty-for-embeddable-terminals)
- [Pinned C API](https://github.com/ghostty-org/ghostty/blob/82232ecde55405559dec29c5466cb9e39938cb41/include/ghostty/vt/terminal.h)
- [Pinned binary snapshot API](https://github.com/ghostty-org/ghostty/blob/82232ecde55405559dec29c5466cb9e39938cb41/include/ghostty/vt/snapshot.h)
- [Pinned build requirements](https://github.com/ghostty-org/ghostty/blob/82232ecde55405559dec29c5466cb9e39938cb41/build.zig.zon)

### Event-stream

Use NessaLabs [`event-stream`](https://github.com/nessalabs/event-stream), pinned
to `66ba7525260040d6265276692088dca6dae0737e`, for the application-event adapter.
Its reader/sink interfaces and memory store are available without a database or
GUI dependency.

The adapter will publish typed output, replay-gap, and completion events through
a caller-supplied sink and stream. Its example will demonstrate publication,
subscription, and reconnect using the real dependency. This adapter adds no
implicit persistence; applications select the sink's external retention policy.
Terminal checkpoint storage is a separate runtime facility, using its built-in
disk store by default, under [ADR 0003](0003-session-parking-and-state-transfer.md).

Keep the PTY's bounded byte replay buffer inside the session. The inspected
event-stream memory store rejects appends when full; it does not automatically
discard oldest bytes. A separate adapter prevents an external event-store stall
from blocking PTY draining or terminating the workload. This distinction must
remain clear in the API: a PTY byte cursor and an event-store cursor identify
different positions.

## 3. Public API and ownership

Organize related concepts in focused modules, with module entry files exporting
their public types. Public domain types receive Rustdoc. Use I-prefixed local
interfaces where an injected implementation provides a useful seam; do not
create interfaces solely to duplicate concrete methods.
Use the domain/application/infrastructure dependency direction, repositories,
boundary DTOs, and error mappings in ADR 0005. Internal crates enforce that
direction while the public `pty-runtime` facade composes default adapters.

| Concern | Planned API | Ownership and behavior |
| --- | --- | --- |
| Commands | `CommandSpec`, `TerminalSize` | Absolute executable/cwd, literal arguments, explicit environment policy and validated dimensions |
| Runtime | `Runtime`, `RuntimeOptions` | Owns a bounded session registry and caller-supplied roots |
| Session | `Session`, `SessionId`, `SessionOptions` | Controls one registered process lifetime; dropping a handle does not kill it |
| Observation | `Attachment`, `AttachPosition` | Independent read cursor; detach/drop only removes an observer |
| Output | `ReplayCursor`, `OutputEvent` | Byte chunks, explicit lost-byte ranges, and final completion |
| Process | `ProcessState`, `ExitStatus`, output-drain status | Separates running, actual OS exit, supervision failure, cancellation request, and complete or truncated drain |
| Emulation | `ITerminalFactory`, `ITerminal`, `TerminalSnapshot`, projection status | Domain terminal state and effects; the default `GhosttyTerminal` implementation and native conversions stay in infrastructure |
| Checkpoint storage | `ICheckpointStore`, `FileCheckpointStore` | Built-in disk storage by default; injected replacements store opaque encrypted bytes under the same commit, read, deletion, and capacity contract |
| Events | `EventStreamPublisher` | Application-owned forwarding through an injected event-stream sink |

### Session identity

- Register a caller-supplied session ID once. A duplicate spawn returns an
  existing-ID error rather than silently executing a second command.
- Look up the existing session to reconnect. Removal requires a finished session
  and is explicit, so retrying a launch cannot accidentally rerun completed work.
- Bind replay cursors to a unique session lifetime as well as a byte offset.
  Reject foreign-lifetime and future cursors.
- Cap registered sessions, including completed sessions retained for replay.
- Bound observers and admitted operations per session and across the runtime.
  Reject excess admission with a typed error rather than accumulating waiters.
- Target at most 4 KiB of runtime-owned idle control state per session. Account
  for reader stacks/scratch, retained bytes, terminal emulation, kernel objects,
  and child-process memory separately. Bound reader count through session admission;
  keep control/reaper workers and optional caches bounded.

### Ownership and shutdown

- Runtime ownership is independent of transports and observers.
- Cancellation admits a termination request and allows the caller to wait for
  a real exit result. Dropping that wait does not undo an admitted request.
  Concurrent requests coalesce; a wait timeout cannot be reported as child exit.
- Graceful runtime shutdown rejects new sessions, signals active process groups,
  escalates after a bounded grace period, and waits for cleanup.
- Runtime drop performs immediate best-effort termination. Shared child-exit
  supervision collects status independently of observers and avoids zombies.
  Do not allocate a permanent waiter thread for every idle session. Validate the
  host's SIGCHLD integration and race-safe reaping before choosing the mechanism.
- Worker abortion must also trigger cleanup. Input or output congestion must
  never prevent cancellation.
- Use unreaped-child identity and synchronization to avoid signalling a reused
  PID after exit. Cover the session's process group and foreground job-control
  group. Document the limit for descendants that create their own sessions or
  otherwise escape those groups.

Limit session persistence to client detachment within one runtime-owner process.
Exclude runtime-owner restart recovery from the initial scope. Terminal snapshots
and retained output cannot establish the presence of a live recoverable process.
Document tested cleanup behavior for abrupt owner death and deliberately escaped
descendants.

## 4. I/O, bounds, and terminal projection

### Process and byte stream

Allocate the PTY through Rust's Unix bindings, configure the child endpoint, and establish
a fresh controlling-terminal session before executing the caller's command.
Use one dedicated host-endpoint reader per live PTY for active and idle
populations. Configure and measure bounded stack and scratch allocations. Prove
that quiet readers can be interrupted for shutdown without waiting for output.
Qualify the full Rust/Ghostty path with this default before release.
Merge stdout/stderr according to normal PTY semantics; return bytes without
assuming UTF-8 or line boundaries.

Input admission has finite queue slots and a finite maximum chunk size. Allocate
owned input only after admission. Write acknowledgements mean bytes were written
to the PTY, not consumed or accepted by the child application. Document partial
writes and cancellation of an already-admitted operation so callers do not
blindly resend authentication input.

Drain output independently of observer speed. Retain a fixed maximum number of
bytes; evict oldest bytes when full. A slow or reattached observer receives a
gap range before the retained suffix. Bound each returned output page and avoid
an accumulating per-observer queue. Track process exit separately from output
completion, including a bounded drain policy for descriptors held by descendants.

Treat replay capacity as a maximum, not an allocation made at spawn. Use a shared
global retention budget so hundreds of quiet sessions do not each reserve a full
history buffer. Any pressure-driven history eviction advances the affected byte
cursor floor and remains visible to observers.

### Idle sessions and parking

Use a configurable 60-second period without PTY output or other model mutation
as initial terminal-parking eligibility. This is a starting policy to qualify,
not a measured optimum. Automatic parking is enabled by default for projected
sessions. The library supplies disk storage; callers can replace the provider
without implementing parking, encryption, or restoration. Cold sessions keep their PTY, process identity, exit
monitoring, and read readiness. Output or operations requiring live model state
trigger restoration. Already-encoded input and compatible snapshot attachment
need not restore the model. Reader placement and observer buffers have separate
activity policies; a periodic one-minute poll must not delay wakeup.

Reclaim optional formatted snapshots, spare buffers, and empty retention pages.
Bound and reuse each reader's scratch buffer; include its per-session cost in
resource admission. Share optional processing buffers through bounded pools. Record logical freed bytes and actual process footprint separately:
returning memory to an allocator does not guarantee prompt OS reclamation.

Preserve unread output and terminal state according to their contracts. Never
close the PTY, stop draining, suspend the child, or discard a partial VT sequence
merely because the session is cold. A complete binary checkpoint may replace a
resident model under the storage and restoration contract in
[ADR 0003](0003-session-parking-and-state-transfer.md). Bound idle scheduling
metadata and background reclamation work; avoid a new timer entry per output chunk.

### Libghostty

Choose raw byte operation or Ghostty projection explicitly at session creation.
For projected sessions, feed each output chunk into the terminal model in stream
order even when no observer is attached. Keep partial UTF-8 and escape-sequence
state across reads.
Route bounded terminal-generated responses through the same serialized PTY
writer without callback reentrancy.

Coordinate OS and model resize in one session operation. On a failed resize,
report a concrete error and keep the published size consistent with the outcome.
Snapshot access must not replay output into a fresh model during reconnect.
Restoration failure is an explicit projection state; preserve process ownership
and bounded raw I/O, without silently replacing the terminal with an empty one.

Expose dimensions, cursor position/visibility, active screen, useful mode flags,
and formatted text/styled content. These values are a headless UI input, not a
GUI renderer or a durable restore format. Use a caller-visible output cursor to
identify which bytes a snapshot includes. Expose binary checkpoint operations
separately, with native-build compatibility, bounded parser continuation,
READY/history completion, and explicit storage policy. Published output and
model-processed cursors may differ when parsing is decoupled; preserve ordered
resize controls and lossless bounded parser staging as specified in ADR 0003.

Set finite scrollback, response, snapshot, and graphics-related limits. Describe
Ghostty's page-granularity scrollback accounting accurately; its configured byte
limit is not an exact whole-process memory cap. Disable terminal image storage
for the initial API unless its resource behavior is explicitly covered.

The few-KiB idle-control target does not include a full Ghostty terminal. Measure
its initial grid, parser, and history costs separately. A raw session must not
pretend that a complete terminal can later be reconstructed from an arbitrarily
truncated replay suffix. For a cold projected session, evaluate Ghostty's bounded
incremental scrollback compression and complete snapshot parking. Qualify the
default parking path's CPU cost, physical reclamation, and wake latency before
release. Measure whether resident-history compression should also be automatic;
the initial OS experiment did not measure either native operation.

### Secrets and path policy

- Redact command fields, environment values, input, output, and snapshot contents
  from Debug and ordinary diagnostics.
- Keep input transient and clear owned input buffers after use. Do not retain
  input in replay, event payloads, command records, or login logs.
- Disable child endpoint echo initially by default. Child programs may alter termios or
  print submitted values; document that their output can therefore contain secrets.
- Default to an empty child environment, with explicit opt-in inheritance,
  removals, and caller overrides.
- Validate canonical working directories against caller-supplied roots. Explain
  that this is a launch-path policy, not a filesystem sandbox for the child.
- Leave authorization of handles and remote messages to the embedding application.

## 5. Implementation sequence and gates

1. **Process and byte ownership — G1.** Implement command validation, PTY setup,
   synchronized signalling and reaping, the bounded registry, dedicated PTY readers,
   input admission, replay, attach/detach, and shutdown. Validate real child
   behavior, byte order, pressure, fairness, and cleanup before adding transports.
2. **Native projection — G2.** Build the pinned Ghostty VT library for the
   package and generate matching Rust bindings. The successful scratch native
   build supplies a starting point; Rust FFI remains unverified. Exercise real
   allocation, incremental input, extraction, resize, replies, and destruction.
   Add ordered session integration, binary round trips, parser continuation,
   READY/history restoration, and incremental compression. Measure projected
   memory separately from raw-reader costs. Native build work can start early,
   but its integration gate depends on the process/byte contract.
3. **Default parking and replaceable storage — G3.** Add the built-in disk store,
   injectable storage interface, runtime-managed encryption, bounded staging,
   generation-checked commit, restoration, and reclaimable pools.
   Qualify concurrent output, cancellation, storage failures, repeated wakeup,
   and memory peaks as an initial-release requirement. Verify both the default
   store and an injected test provider. Keep dedicated readers while terminal models park.
4. **Adapters and release — G4.** Add the caller-owned event publisher, typed
   events, bounded retry behavior, and interactive/reconnect examples. Verify
   sink stalls and replay gaps. Review FFI safety, ownership, races, bounds, and
   redaction. Run standard checks, the executed platform matrix, performance
   workloads, and sustained stability qualification; document exact outcomes.

The concrete gates and race scenarios are in
[Integration and release qualification](0004-integration-and-release-qualification.md).
Bounded dynamic reader handoff is a later optimization with its own platform
proof and measurements; it is not required for the dedicated-reader baseline.

## 6. Validation matrix

All process fixtures are Rust programs or explicitly selected local system
utilities. Authentication-like fixtures use synthetic URLs, prompts, and codes.
Do not run real login commands, use credentials, call models, or consume paid
resources during validation.

| Area | Required evidence |
| --- | --- |
| Native integration | A real static Ghostty build and exercised FFI; no stub backend |
| Byte I/O | Bidirectional raw bytes, chunk boundaries, partial writes, stderr/stdout PTY merge |
| Authentication-like flow | Prompt, synthetic submitted code, success marker, real exit status, input absent from replay when echo is off |
| Unicode and VT | Split multibyte characters and escape sequences, combining/wide text, cursor movement, erase, color formatting, alternate screen |
| Terminal replies | Child sends a terminal query and receives the Ghostty-generated response |
| Resize | Child observes new dimensions and terminal snapshot agrees; invalid dimensions rejected |
| Exit and drain | Zero/nonzero exit, signal death, final output after exit, bounded completion with inherited descriptors |
| Cancellation | Cooperative and signal-ignoring children; process-group descendants; cancellation during blocked input |
| Detach and reattach | Same PID/workload continues, ordered replay, no duplicated launch or replay advancement on a cancelled read |
| Replay | Exact byte cap, slow-reader gaps, multiple independent observers, foreign/future cursor rejection |
| Ownership | Handle drop, last observer drop, explicit shutdown, owner drop, worker/runtime teardown, no unreaped child |
| Backpressure | Input saturation stays bounded; output continues without readers; cancellation bypasses a full queue |
| Fairness and latency | A flooding session or slow snapshot/event consumer does not starve input, cancellation, resize, or other sessions |
| Sustained stability | Repeated lifecycle/reconnect races and soak workloads meet the resource and correctness gates in the performance plan |
| Cold sessions | Automatic wakeup, reclaimed optional memory, exact replay gaps, preserved parser continuation, and process exit handling while cold |
| Environment and paths | Empty/inherited environments, removal/override order, literal arguments, cwd boundaries, symlink resolution |
| Diagnostics | Sensitive synthetic markers absent from Debug/errors; no input persistence |
| Event-stream | Real publication and replay-to-live subscription; sink rejection/cancel/retry; gaps preserved across reconnect |

Planned standard checks are `cargo fmt --check`, `cargo clippy --locked
--all-targets --all-features -- -D warnings`, `cargo test --locked --all-targets
--all-features`, a build/test without the optional event-stream feature, and
Rustdoc with warnings denied. Run a manual interactive smoke test in a real
terminal. Add Linux CI, attempt Linux compilation locally where toolchains
permit, and label Linux execution unverified until it actually runs.

## 7. Workspace-sdk integration sketch

The embedding host holds one runtime for the lifetime of its native backend.
Map an interactive operation's idempotency key to a session ID, create the session
once, and reconnect with lookup thereafter. Adapt command arguments, cwd,
environment overrides/removals, and timeouts without shell interpolation.

Return a process reference backed by that session. Deliver output through an
attachment or the event-stream adapter, accept transient byte input through an
authorized control path, and inspect the actual session exit status. A separate
application command can verify authentication state after the interactive
process completes.

The sketch will identify these adapter changes without modifying workspace-sdk.
The package itself has no knowledge of a particular authentication provider.

## 8. Completion criteria

The implementation is complete only after the pinned native integration builds,
the package and examples compile, behavioral tests pass on each claimed platform, and
the documentation states exact ownership, replay, security, and platform limits.
Performance qualification and stability qualification must also pass on the
recorded baseline host; passing unit tests alone is insufficient.
Assess the gates in ADR 0004. The target matrix includes macOS and Linux on both
architectures; narrow any initial support claim to targets actually qualified.
Record configured-but-unrun checks separately from successful evidence. Any
native build failure must include its concrete tool/dependency error and the
step it prevents; do not substitute an unimplemented backend.
