---
id: "parking"
title: "Park and restore a quiet model"
kind: "flow"
status: "implemented"
summary: "Only a committed current checkpoint can replace the live model; usable
  is not full history."
sources:
  [
    "crates/domain/src/projection/policy.rs",
    "crates/application/src/projection/io.rs",
    "crates/application/src/projection/queue.rs",
    "crates/domain/src/terminal/checkpoint.rs"
  ]
sourceRevision: "c68837e2b7bf1c9009e1a4587a1a58107ec0425c"
diagramLinks: {}
parent: "pty"
model: { "spec": "tla/ParkCommit.tla", "config": "tla/ParkCommit.cfg" }
diagramDetails:
  [
    {
        "diagram": 0,
        "node": "Parked",
        "section": "Keep the live model until publication is safe"
      },
    {
        "diagram": 0,
        "node": "Usable",
        "section": "Usable is not complete history"
      },
    {
        "diagram": 0,
        "node": "Resident",
        "section": "Usable is not complete history"
      }
  ]
modelCheck:
  sourceRevision: "c68837e2b7bf1c9009e1a4587a1a58107ec0425c"
  checker: tlc-1.7.4
  date: 2026-11-03T09:08:38.054Z
  config: tla/ParkCommit.cfg
  modelDigest: 520cdff481ca062025a6e22ba2bbe0dac11fa0150fe955dcc186edd42a064260
  states: 20
  held:
    - PublishedBeforeRelease
    - CurrentAttempt
    - OpenAtRelease
  broke: []
  unchecked: []
---
# Let a quiet terminal release native memory

Projected sessions become eligible to park after 60 seconds without output or
another model mutation by default. Parking replaces the terminal model with an
encrypted checkpoint; the child, reader, input and cancellation remain owned.
This policy threshold is not a measured optimum or total-memory guarantee.

```mermaid
stateDiagram-v2
    Resident --> Parking: Eligible and processed catches published
    Parking --> Parked: Storage published and attempt still current
    Parking --> Resident: Changed activity or rejected or uncertain storage
    Parked --> Restoring: Live model needed
    Restoring --> Usable: Active state ready and history incomplete
    Usable --> Resident: Source finished with progress reported
    Restoring --> Failed: Restore unavailable or incompatible
    Resident --> Closing: Owner closes
    Parking --> Closing: Owner closes
    Restoring --> Closing: Owner closes
    Closing --> Closed: Accepted work relinquishes ownership
```

## Keep the live model until publication is safe

An attempt records operation generation, activity, processed byte position and
control generation. Slow storage runs outside native ownership. After a successful
commit, the application checks those values and pending work together before
releasing the live model. Output arriving meanwhile invalidates the attempt;
its unused committed object goes to cleanup. Failed storage keeps the model.
An uncertain worker outcome may already have stored ciphertext: it remains
charged as unreclaimed instead of being treated as free capacity.

## Usable is not complete history

Restoration authenticates compatible state and creates a usable active screen,
then interleaves bounded older-history steps with live work. Live mutations may
make old pages inapplicable. Inspect history progress and skipped-page count;
Resident after finished decoding does not erase skipped history. Failed restore
preserves process ownership and bounded raw output, but cannot promise a usable
terminal. A compatible parked checkpoint can serve transfer without restoring the
server model. Already-encoded input need not restore it either.

ParkCommit models one attempt, one independent activity change, publication and
close. It checks release requires published storage, unchanged activity and open
ownership. It abstracts operation/control identity into that one activity token;
crypto, storage I/O, retry timing, native destruction and history are unmodeled.
