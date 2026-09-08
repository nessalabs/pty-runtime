# ADR 0003: Independent parking and terminal state transfer

**Status: implementation in progress.** The real Rust/native wrapper and encrypted
checkpoint adapters have contract tests. Integrated parking policies and full G3
proof remain pending; see the [ledger](../verification/requirements.md).

Extend the [architecture plan](0001-pty-runtime.md) with separate policies for
terminal storage, reader placement, and observer buffers. These are planned
behaviors. The [existing experiment](../experiments/0001-pty-speed-and-memory.md)
measures fixed reader models and synthetic caches; it does not measure native
snapshot restoration, compression, dynamic handoff, or client synchronization.
The [native follow-up](../experiments/0002-native-terminal-parking.md) now measures
compression, disk parking, allocator reclamation, and snapshot timing through
the C API. Integrated handoff and client synchronization remain unmeasured.

## Decision

Use one dedicated reader per live PTY as the initial default, bounded by session
admission. Quiet sessions retain their reader; terminal parking does not release
its thread or stack. The accepted latency/memory tradeoff is recorded in
[Experiment 0003](../experiments/0003-cross-platform-concurrent-workloads.md). Automatically park eligible projected terminals in binary
snapshots by default. Ship a disk checkpoint store and allow callers to inject a
replacement. The runtime owns parking, encryption, and restoration independently
of the provider. Models remain live during activity and may use compression
while resident. Reclaim unused observer buffers independently of both choices.

| Resource | Quiet state | What needs it again |
| --- | --- | --- |
| PTY reader | Dedicated reader remains blocked with bounded stack/scratch; no periodic polling | Output wakes the reader; shutdown must interrupt it promptly |
| Terminal model | Complete snapshot in the default disk store or injected provider; retain resident state if parking cannot complete | PTY output, resize, or another operation requiring live model state |
| Observer buffers | Cursor and subscription metadata, with spare buffers released | Data transfer allocates bounded buffers on demand |

Start terminal parking eligibility after 60 seconds without PTY output or other
model mutation. This threshold is a heuristic to measure. Sending already-encoded
input bytes does not itself require a live model. Input encoding that consults
terminal modes may require one. An attachment alone need not restore the server
model when a compatible saved snapshot can satisfy it. Keep process exit,
cancellation, input delivery, and output readiness operational in every state.

Caller performance policy must account for agent workloads as well as visible
human clients. Absence of an observer does not prove throughput is unimportant.

## Verified native capabilities

The existing Ghostty pin, `82232ecde55405559dec29c5466cb9e39938cb41`, already
contains `ghostty_snapshot_encode` and decoder operations `ready`, `next`, and
`decode`. READY restores usable terminal state and unfinished parser input;
subsequent calls restore older history. The format is versioned but still has
no binary-compatibility guarantee. Keep formatted `TerminalSnapshot` output
distinct from this binary checkpoint. See the [pinned snapshot API](https://github.com/ghostty-org/ghostty/blob/82232ecde55405559dec29c5466cb9e39938cb41/include/ghostty/vt/snapshot.h).

Enable bounded continuation tracking before feeding bytes if checkpoints must
work while UTF-8 or a VT sequence is unfinished. If tracking becomes unavailable,
keep the model resident and retry at a valid boundary; do not discard parser
state. Set explicit continuation limits and retain tracking after restore for
subsequent automatic parking. The [terminal API](https://github.com/ghostty-org/ghostty/blob/82232ecde55405559dec29c5466cb9e39938cb41/include/ghostty/vt/terminal.h)
also exposes caller-scheduled incremental history compression. Compression
preserves contents, can decline unprofitable pages, and requires serialized
access to the terminal. It is separate from snapshot storage.

The native follow-up built this exact library and exercised complete snapshots,
partial parser state, compression, and restoration in scratch C fixtures. It
also verified binary round trips, resumed parsing, resize, and replies. The
[upstream C example](https://github.com/ghostty-org/ghostty/blob/82232ecde55405559dec29c5466cb9e39938cb41/example/c-vt-snapshot/src/main.c)
documents the same API. The Rust wrapper now has native contract tests; complete
integrated qualification remains work for the implementation phase.

## Terminal parking and restoration

Capture a checkpoint at an exact processed-output cursor and control generation.
Complete and validate the stored snapshot before releasing the live model.
Include all state required by the terminal contract and preserve the ordinary
raw replay contract separately: terminal history is not an exact byte log.
Associate the attempt with its session lifetime and an operation generation.
Commit storage outside exclusive native access, then atomically verify that the
attempt is current and no pending activity requires the model before freeing it.
Output or another mutation before commit invalidates the parking attempt; retain
the live model and clean up the unused checkpoint. A stale completion must never
replace newer state or revive a removed session.

Do not require in-memory compression immediately before parking: the native
experiment found identical binary snapshot sizes with and without that step.
Use compression when history will remain resident. Store compression buffers in
reclaimable storage; freeing default-allocator buffers left substantial charged
memory in the experiment. Prototype shared pages that pack small buffers, then
release empty pages, instead of adopting the experiment's per-allocation mapping
threshold unchanged. Its lower threshold improved parking but increased resident
memory through page rounding.

Encoding is synchronous and forbids concurrent model mutation for the entire
call. READY does not shorten that encoder ownership interval. Create an immutable,
bounded checkpoint on a bounded worker before giving it to a slow consumer.
Keep PTY reads serviced through bounded staging during that interval; if the
staging budget fills, apply explicit per-session backpressure and record the
stall. Do not hold a global lock or connect the encoder directly to an arbitrarily
slow network writer. On failed encoding or storage, retain the original model.
The encoder callback must accept bytes into bounded staging or fail promptly;
it must not wait for slow storage while holding the terminal. Keep a separate
budget for staging, pending commits, and immutable snapshot pins.

On new output, retain the bytes and restore READY before feeding them into the
model in order. Rebind host callbacks and enforce the configured native limits
before processing live bytes. Restore older history with bounded work between
model operations. Maintain the saved source until restoration finishes; expose
history completeness, because live mutations can make an old page inapplicable.
Do not claim full history restoration when only READY has completed.
If a committed checkpoint cannot be restored, report projection unavailability
and preserve process ownership and bounded raw I/O. Do not reset the terminal
silently. Keep queued parser bytes lossless up to their cap, then apply the
session's backpressure policy. Cancellation and exit supervision remain usable.

### Default store and replacement contract

Ship `FileCheckpointStore` as the default implementation of `ICheckpointStore`.
Creating the runtime requires no custom storage implementation. The default
store uses a private directory for that runtime lifetime under the OS temporary
directory; callers may configure another root and finite storage limits.
Keep checkpoint names opaque and permissions private to the owner.

The runtime serializes and authenticates/encrypts checkpoints before passing
bytes to the provider. It owns compatibility metadata, per-runtime keys, parking
eligibility, byte/control positions, storage reservations, restoration, and
cleanup scheduling. A provider stores opaque encrypted bytes and does not need
Ghostty knowledge. Replacing it must preserve the same public session behavior.

The provider contract requires immutable checkpoint identifiers, all-or-error
commit, bounded reads, deletion, typed failures, and well-defined cancellation.
A successful commit makes the complete checkpoint readable until the runtime
releases it; a provider may not silently evict a parked session's only copy.
Enforce runtime admission, byte budgets, and snapshot pins independently of the
provider, including in-flight writes and retained sources during restoration.
Native encoder callbacks never call slow storage directly.

Use authenticated encryption and a vetted implementation; native codec CRCs
provide neither encryption nor authenticity. Generate and retain encryption keys
for the owner runtime's lifetime. Owner-restart recovery and persistent key
management remain outside the initial scope. Record compatibility with the exact
native build. On normal cleanup, delete owned checkpoints and release keys;
bound abandoned-file cleanup within the store's own namespace after crashes.

If storage is full or unavailable, retain the live or compressed model, report
the parking failure, and retry under a bounded policy. Do not lose terminal state
to meet a memory target. Default parking applies to projected sessions; raw
sessions have no native model to park. Snapshot files preserve terminal state,
not running processes across owner termination.

The quiet footprint still includes session metadata, kernel PTYs, child
processes, dedicated reader stacks/scratch, retained raw output, and potentially filesystem cache. Few-KiB control
state is a separate budget from total machine memory.

## Reader placement and ownership

Dynamic placement is a qualification-gated extension to the dedicated baseline.
The initial implementation retains dedicated readers for quiet sessions. A future
policy may move quiet sessions onto shared readiness to reclaim reader resources;
bound both strategies globally and use hysteresis to avoid thread churn. Do not
claim those savings until a handoff experiment verifies them.

Transfer ownership only after the old reader acknowledges it has stopped and
has published all bytes it already read. Exactly one reader owns the descriptor
at a time; an ownership generation rejects stale readiness events. The new owner
must check for pending data immediately. Demonstrate that a silent blocked
reader can be interrupted promptly without closing the session's PTY or waiting
for output. Descriptor blocking-mode changes must preserve the input writer's
bounded behavior. These are unresolved platform mechanics to prove in a small
handoff experiment before implementing the policy in the library.

The fixed-reader results justify testing this tradeoff, not a claim that hybrid
placement is already faster. Measure mixed populations, handoff cost, thread
creation rate, wake latency, fairness, and charged memory together.

## Snapshot plus ordered live output

Provide backend primitives for one authoritative terminal and multiple independent
observers. A compatible consumer restores a binary checkpoint, then receives
ordered original PTY bytes plus ordered controls such as resize. It owns its
viewport. The embedding transport supplies framing and client compatibility;
the library does not implement a graphical client or network server.

Distinguish the latest published byte cursor from the cursor already processed
by the server model. A checkpoint records the latter and its control generation.
Retain the intervening ordered events while constructing the snapshot so that
handoff has neither missing nor duplicated updates. A slow consumer that loses
the retained range must resynchronize explicitly. A parked server can provide
its immutable snapshot without restoring its own model, subject to that same
cursor and compatibility contract.

Let raw output delivery proceed independently of model parsing only with a
bounded, lossless parser backlog. Reserve that storage separately from evictable
observer replay; observer gaps cannot excuse missing server parser input. At the
parser backlog limit, pause reads for the affected session until parsing catches
up, allowing OS backpressure while other sessions and control operations remain
serviceable. This is overload handling, never an idle-parking technique.

Serialize user input and authoritative terminal-generated replies through one
PTY writer. Replicated clients must not each send duplicate query replies. Match
terminal capabilities and ordered control state as well as byte content; a byte
tee alone cannot establish equivalent terminal models.

## Remaining integrated qualification before enabling policies

The native follow-up establishes the codec and reclamation tradeoff on this
macOS host. The following gates still apply to the actual session implementation,
including concurrent output, temporary peaks, storage failures, and host integration.
The deterministic interleavings, failure outcomes, and feature gates are defined
in [ADR 0004](0004-integration-and-release-qualification.md).

| Experiment | Required evidence |
| --- | --- |
| Native memory | Empty, representative filled, compressed, parked, and restored models; logical bytes, allocator capacity, charged footprint, disk bytes, and temporary peak memory |
| Snapshot timing | Encoding, READY, full history completion, and first post-wake output measured separately; distinguish in-memory codec work from cached and uncached storage I/O |
| Correctness | Split UTF-8 and VT sequences, alternate screen, modes, resize, replies, continuation-limit exhaustion, malformed snapshots, failed storage, and repeated park/restore cycles |
| Reader handoff | Quiet blocked reader, output during transfer, concurrent input/cancel/exit, no lost wakeup, no duplicate reads, and bounded worker count |
| Observer scaling | Many observers, repeated attachment, stalled consumers, snapshot-to-live races, and return of spare-buffer memory after activity |

Compare all strategies at equal terminal dimensions, retained history, active
output rate, and observer count. Report input bytes, resident terminal bytes,
encoded snapshot bytes, and time to READY separately. A microsecond codec result
must not be presented as complete disk wake latency.

Upstream already distinguishes full decode from READY-only decode in its
[snapshot benchmark](https://github.com/ghostty-org/ghostty/blob/82232ecde55405559dec29c5466cb9e39938cb41/src/benchmark/TerminalSnapshot.zig).
Use that distinction in our fixture. Do not turn reported external timings or
fixed-reader measurements into unrun native acceptance results.
