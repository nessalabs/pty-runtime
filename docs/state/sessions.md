---
id: "sessions"
title: "Start reconnect and finish"
kind: "flow"
status: "implemented"
summary: "Runtime owns the child; handles observe it, and completion preserves
  exit and drain separately."
sources:
  [
    "src/runtime.rs",
    "crates/application/src/runtime/owner.rs",
    "crates/application/src/runtime/session.rs",
    "crates/domain/src/session/mod.rs"
  ]
sourceRevision: "c68837e2b7bf1c9009e1a4587a1a58107ec0425c"
diagramLinks: {}
parent: "pty"
model: { "spec": "tla/CompletionFacts.tla", "config": "tla/CompletionFacts.cfg" }
diagramDetails:
  [
    { "diagram": 0, "node": "Finished", "section": "Finish is a pair of facts" },
    { "diagram": 0, "node": "Running", "section": "Who owns unfinished work" }
  ]
modelCheck:
  sourceRevision: "c68837e2b7bf1c9009e1a4587a1a58107ec0425c"
  checker: tlc-1.7.4
  date: 2026-11-03T09:08:50.750Z
  config: tla/CompletionFacts.cfg
  modelDigest: 005b04ed6d2bb7ae5e9174450d8bd534dc49343684ec82125619f21c5d885cb4
  states: 11
  held:
    - BothFacts
    - NoInventedExit
  broke: []
  unchecked: []
---
# Start once and reconnect to the same child

Create Runtime before starting worker threads: the built-in helper-image staging
forks, and other threads' platform locks can deadlock that construction. The
[embedding guide](../usage.md) and [interactive example](../../examples/interactive.rs)
show the supported composition. Choose raw or projected SessionOptions at spawn;
mode is fixed for that lifetime. The default ghostty feature supplies projection
services. A raw-only build or composition without them returns Unsupported for
projected spawn; use `with_projection_adapters` to supply compatible services.

```mermaid
stateDiagram-v2
    [*] --> Registered: Reserve caller ID and fresh lifetime
    Registered --> Running: Process backend accepts spawn
    Registered --> Removed: Failed admission rolls back registration
    Running --> Running: Lookup or attach or detach
    Running --> Facts: Exit or supervision failure observed
    Facts --> Finished: Drain also finishes
    Running --> Drained: Drain finishes first
    Drained --> Finished: Exit or supervision failure observed
    Finished --> Removed: Explicit forget
```

Use `spawn(id, command, options)` once. An existing registered ID returns
ExistingSession, including after completion; use `lookup(id)` to observe it.
Reserve a business-level ID in the host if several requests could race. The
library does not persist that registry across owner restart. Spawn errors roll
back the reserved entry; no error is manufactured into a successful child exit.

## Finish is a pair of facts

`wait()` reserves an observer and waits for a supervision outcome **and** a drain
outcome. Read `Completion.status`: `exit` is an actual reaped exit; supervision or
admission failures are separate. Drain can be EOF, truncated or failed. A child
can exit while descendants keep output open; a drain deadline is not proof of
complete output. Finished therefore means observation is settled, not successful.
After recording the result, call `forget(id)` to release a completed registry
entry. Forget refuses unfinished work. Reusing the ID creates a new lifetime;
old handles and cursors still address the old one.

## Who owns unfinished work

Dropping Session, Attachment or a completion wait does not terminate the child.
Keep Runtime alive across ordinary viewer disconnects. Explicit `shutdown()`
rejects new spawns, shuts down the backend, waits admitted spawns and projection
workers. Runtime Drop uses immediate backend shutdown. Inspect cancellation and
completion evidence when the outcome matters; process-group escape and hostile
unbounded forking are outside the documented Guardian cleanup scope.

The bounded CompletionFacts model checks the required pair and that failure does
not invent exit. It excludes OS signaling, registration rollback, waiting time,
Guardian algorithms and eventual cleanup. Existing tests exercise those separately.
