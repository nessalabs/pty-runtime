---
id: "projection"
title: "Observe the terminal model"
kind: "flow"
status: "implemented"
summary: "Published bytes may be ahead of processed state; a failed native feed is not replayable."
sources: ["crates/application/src/runtime/session_projection.rs", "crates/application/src/projection/output.rs", "crates/application/src/projection/native.rs", "crates/infrastructure/src/terminal/mod.rs"]
sourceRevision: "c68837e2b7bf1c9009e1a4587a1a58107ec0425c"
diagramLinks: {}
parent: "pty"
diagramDetails: [{"diagram": 0, "node": "Failed", "section": "Failed feed is not a retryable chunk"}]
---

# Read a screen with its processing boundary

A projected session maintains one authoritative terminal model. Ask
`projected_view()` for an admitted copy of its active grid and inspect
`projection_status()`: published is output admitted to parser staging; processed
is output successfully applied. A view is not automatically current through all
published bytes. Raw sessions have no projection to observe.

```mermaid
flowchart TD
    Output[PTY output] --> Stage[Bounded parser staging]
    Stage --> Feed[Serialized native feed]
    Feed -->|All slices succeed| Commit[Commit status and processed boundary]
    Commit --> View[Copied active grid]
    Commit --> Reply[Authoritative replies through PTY writer]
    Feed -->|Any prefix may have applied| Failed[Projection failed]
    Failed --> Raw[Process and bounded raw I/O remain independent]
```

## Failed feed is not a retryable chunk

Native feed can mutate a prefix before failing. The worker retains original bytes
for diagnosis, fails projection and does not replay them into that mutated model.
No empty replacement is silently created. Check failure and retain raw output for
diagnosis; no general in-place recovery from a failed projection is promised.
At the parser staging cap, backpressure applies to this session. Cancellation and
supervision have separate ownership.

Snapshots retain quota until dropped; copying their borrowed contents makes a
separate caller allocation. Terminal modes preserve distinctions needed to encode
mouse/input events. Clipboard/desktop policy is the embedding application's concern.
A native crash can end the owner process; a Rust wrapper is not process isolation.

## History belongs to the terminal port

`ITerminal::history(start, count)` is a bounded read-only range query. The public
Session facade exposes active views/checkpoints/transfers, not a history method.
The demo owns an ITerminal directly and uses its history API. Row indices are
relative to the current retained window, not stable historical IDs; eviction and
incremental restoration change what can be read. Counts help re-anchor, but do not
prove all prior history is present. Viewport position belongs to each consumer.
See [ADR 0006](../adr/0006-scrollback-projection.md) and [parking](parking.md).
