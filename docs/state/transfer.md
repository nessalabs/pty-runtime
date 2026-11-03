---
id: "transfer"
title: "Restore a viewer without missing controls"
kind: "flow"
status: "implemented"
summary: "A checkpoint and continuation share one boundary; lost continuation
  requires a new transfer."
sources:
  [
    "crates/domain/src/projection/transfer.rs",
    "crates/application/src/projection/transfer.rs",
    "crates/application/src/projection/journal.rs",
    "crates/application/src/projection/snapshot.rs"
  ]
sourceRevision: "c68837e2b7bf1c9009e1a4587a1a58107ec0425c"
diagramLinks: {}
parent: "pty"
model: { "spec": "tla/TransferSeal.tla", "config": "tla/TransferSeal.cfg" }
diagramDetails:
  [
    {
        "diagram": 0,
        "node": "End",
        "section": "End and loss need different decisions"
      },
    {
        "diagram": 0,
        "node": "Loss",
        "section": "End and loss need different decisions"
      }
  ]
modelCheck:
  sourceRevision: "c68837e2b7bf1c9009e1a4587a1a58107ec0425c"
  checker: tlc-1.7.4
  date: 2026-11-03T09:08:54.683Z
  config: tla/TransferSeal.cfg
  modelDigest: d127ebf636723d9631395d6043d47d45889da70ff5e6d54241c9073abe54d3b5
  states: 27
  held:
    - FrozenPrefix
    - EndHasEvidence
  broke: []
  unchecked: []
---
# Rebuild a compatible viewer from a checkpoint

Call `begin_transfer()` on a projected session. Restore its checkpoint into a
compatible terminal engine, then consume its observer from the returned boundary.
Apply original output and successful resize events in sequence. Discard replies
that the viewer's model generates: only the authoritative server writes them to
the child. A raw byte cursor cannot substitute for this sequence because resize
changes state without consuming output bytes.

```mermaid
flowchart TD
    Capture[Checkpoint and observer at one boundary] --> Restore[Compatible consumer restores checkpoint]
    Restore --> Read[Read explicit continuation cursor]
    Read --> Event[Apply output or resize then advance cursor]
    Event --> Read
    Read --> Pending[No next event yet]
    Pending --> Read
    Read --> Loss[ResyncRequired]
    Loss --> Capture
    Read --> End[Immutable final applied prefix]
```

## End and loss need different decisions

`TransferEvent::after().cursor` is the next position after applying that event.
Read does not advance the caller's cursor; cancelling a wait only clears its
notification. If retention is lost, discard the partial reconstruction and request
a fresh transfer. Do not skip to the oldest retained event on an old checkpoint.

End describes the final successfully applied prefix. Inspect its drain and
projection_failure: a failed prefix is not parser catchup or a successful process
exit. Pending is not End. The first End remains immutable; later observations do
not upgrade an earlier failure. Runtime closure rejects old observers.
Checkpoint pins and returned events retain quota until dropped; slow consumers
can exhaust their own allowed observations without creating an unbounded queue.

TransferSeal checks one bounded stream's sealed boundary cannot advance and a
successful ending needs a drain fact. It includes a failed-prefix ending without
drain. It does not prove checkpoint compatibility, payload equivalence, reply
suppression, journal retention or multi-consumer transport correctness.
