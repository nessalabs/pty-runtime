---
id: "program-status"
title: "Read a program’s reported status"
kind: "flow"
status: "implemented"
summary: "Untrusted reports commit only after a whole terminal input chunk succeeds."
sources:
  [
    "crates/domain/src/terminal/program_status/store.rs",
    "crates/domain/src/terminal/program_status/report.rs",
    "crates/application/src/projection/program_status.rs",
    "crates/application/src/projection/output.rs"
  ]
sourceRevision: "c68837e2b7bf1c9009e1a4587a1a58107ec0425c"
diagramLinks: {}
parent: "pty"
model: { "spec": "tla/StatusCommit.tla", "config": "tla/StatusCommit.cfg" }
diagramDetails:
  [
    {
        "diagram": 0,
        "node": "Failed",
        "section": "Plan and terminal acceptance must agree"
      },
    {
        "diagram": 0,
        "node": "Commit",
        "section": "Plan and terminal acceptance must agree"
      }
  ]
modelCheck:
  sourceRevision: "c68837e2b7bf1c9009e1a4587a1a58107ec0425c"
  checker: tlc-1.7.4
  date: 2026-11-03T09:08:42.644Z
  config: tla/StatusCommit.cfg
  modelDigest: dbd05117072a80571fa69cb27a2208ebac810e220e2e3885280b2855049b0983
  states: 17
  held:
    - LocalPlan
    - CurrentPlan
    - WholeNativeAcceptance
  broke: []
  unchecked: []
---
# Display a reported task state without treating it as authority

A program can emit a bounded OSC 7501 report such as working, blocked, idle, done
or error. `Session::program_status()` copies the latest processed reports from a
projected session; raw sessions return None. Reports are untrusted application
text, not proof of process completion, authentication or approval. Display them
as plain text with appropriate escaping.

```mermaid
flowchart TD
    Bytes[Original output chunk] --> Plan[Scan immutable plan]
    Plan --> Feed[Feed all native slices]
    Feed -->|Success and current plan| Commit[Commit records and parser continuation]
    Feed -->|Any prefix fails| Failed[Fail projection without replay]
    Plan --> Drop[Discarded plan changes nothing]
    Commit --> Snapshot[Admitted status snapshot]
```

## Plan and terminal acceptance must agree

The store rejects a plan from another store or an older committed generation.
The application calls commit only after all native feed slices succeed. If a
prefix fails, records do not pretend the whole chunk applied, and replay into the
mutated model is forbidden. Only the authoritative projected session emits the
fixed capability reply; raw forwarding and observers do not answer queries.

Reports replace an ID's fields; ancestor application inheritance resolves on read.
Clearing a parent clears descendants. Prompt or process exit clears working and
blocked records, while idle/done/error may remain. Final output drain clears
working/blocked again after buffered reports. EOF alone is not process exit.
The store persists across parking within the session; a status read does not
restore the native model. It observes processed output, not a barrier waiting for
all published bytes. Snapshot memory remains charged until dropped.

StatusCommit models two stores, one scanned generation, an independent competing
commit and all-slices acceptance. It checks committed plans are current, local and
native-accepted. The last condition is application orchestration, not a guard in
ProgramStatusStore::commit itself. Parser grammar, caps, eviction and reply ordering
are source/test-reviewed rather than modeled.
