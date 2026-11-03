---
id: "event-forwarding"
title: "Retry an uncertain event publication"
kind: "flow"
status: "implemented"
summary: "Keep the same publisher so its pending identity and payload survive an
  uncertain append."
sources:
  [
    "crates/infrastructure/src/event_stream/publisher.rs",
    "crates/infrastructure/src/event_stream/codec.rs",
    "examples/event_stream_reconnect.rs"
  ]
sourceRevision: "c68837e2b7bf1c9009e1a4587a1a58107ec0425c"
diagramLinks: {}
parent: "pty"
model:
  {
    "spec": "tla/PublicationReceipt.tla",
    "config": "tla/PublicationReceipt.cfg"
  }
diagramDetails:
  [
    {
        "diagram": 0,
        "node": "Pending",
        "section": "Retry the retained publication"
      }
  ]
modelCheck:
  sourceRevision: "c68837e2b7bf1c9009e1a4587a1a58107ec0425c"
  checker: tlc-1.7.4
  date: 2026-11-03T09:08:34.757Z
  config: tla/PublicationReceipt.cfg
  modelDigest: a095b65fa26879a0bc0aa86dcaf64cf91f50cbdb8e645dd66d2cf84be3bf3d71
  states: 7
  held:
    - ValidatedAcknowledgement
    - StablePendingIdentity
  broke: []
  unchecked: []
---
# Forward output to a caller-owned event sink

Enable the optional event-stream feature, resolve a stream incarnation, and create
EventStreamPublisher from an admitted raw attachment and your sink. Drive
`publish_next()` yourself. It creates no background worker or implicit store.
The [reconnect example](../../examples/event_stream_reconnect.rs) uses the real
pinned dependency to demonstrate replay, gaps and subscription reconnect.

```mermaid
stateDiagram-v2
    Idle --> Pending: Read source and prepare identity and payload
    Pending --> Pending: Sink rejects or outcome uncertain or wait cancelled
    Pending --> Acknowledged: Receipt matches event stream version and increasing offset
    Acknowledged --> Idle: More source events
    Acknowledged --> Complete: Completion receipt accepted
```

## Retry the retained publication

A lost reply may mean the sink committed. Keep the publisher and call publish_next
again to retry its exact pending identity/schema/payload; pending_event_id helps
reconciliation. The source attachment may already have advanced, but the
acknowledged byte cursor advances only after a validated receipt. An invalid
receipt leaves pending state intact. Dropping the publisher loses that retry
identity and does not undo accepted sink work. A replacement publisher does not
promise deduplication of the old uncertain append.

Use event-store cursors to resume event subscriptions; PTY byte cursors identify
original output positions and have a different lifetime. One publisher retains
one bounded pending record. External queues and retention have separate budgets.
A stalled sink can let raw replay age out, producing an exact gap later; it does
not become authoritative parser staging. Retrying capacity errors is a caller
policy, not silent eviction of the sink.

PublicationReceipt models one prepared record and independent matching/mismatching
receipts. It checks acknowledgement requires a matching receipt and uncertain
attempts retain pending identity. Stream/version/offset validation is abstracted
into receipt validity; sink idempotency and crash persistence are not proved.
