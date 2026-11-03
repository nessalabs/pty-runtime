---
id: "inventory"
title: "Find every feature owner"
kind: "guide"
status: "reference"
summary: "Source ownership and verification scope across the facade, core, adapters, demo and tools."
sources: ["src/lib.rs", "Cargo.toml", "AGENTS.md", "docs/features/README.md"]
sourceRevision: "c68837e2b7bf1c9009e1a4587a1a58107ec0425c"
diagramLinks: {}
parent: "pty"
---

# Find the owner before changing behavior

| Feature or surface | Source owner | Atlas explanation and evidence scope |
| --- | --- | --- |
| Composition, injected adapters, raw/projected choice | src/runtime.rs, src/adapters.rs | [Session](sessions.md), source review |
| ID reservation, lookup, explicit forget, shutdown | application/runtime/owner.rs, infrastructure/registry | [Session](sessions.md), bounded completion model only |
| Command/environment/cwd validation and Guardian construction | domain/process, infrastructure/process, helpers/guardian | [Input](input.md), implementation gate; roots are not sandboxing |
| Ordered partial writes, resize, cancel and process supervision | application/runtime/session.rs, infrastructure/process | [Input](input.md), implementation tests; OS cleanup unmodeled |
| Replay, gaps, observer cursor and completion wait | domain/replay.rs, application/runtime/attachment.rs | [Replay](replay.md), admission model plus tests |
| Native feed, replies, active screen, failure | application/projection/output.rs, infrastructure/terminal | [Projection](projection.md), source and native gate |
| Read-only scrollback | application/terminal port, infrastructure/terminal, demo session | [Projection](projection.md), no Session history facade |
| Parking, restore, history progress, worker teardown | domain/projection, application/projection | [Parking](parking.md), release guard model only |
| Checkpoint plus ordered output/resize transfer | domain/projection/transfer.rs, application/projection/journal.rs | [Transfer](transfer.md), seal model only |
| Encrypted files, private arena, abandoned cleanup | infrastructure/checkpoint, application/checkpoint | [Storage](storage.md), implementation gate; no crypto proof |
| Program status parser/store, native acceptance and snapshots | domain/terminal/program_status, application/projection | [Status](program-status.md), transactional model only |
| Event publication/codec, receipt validation, retries | infrastructure/event_stream | [Forwarding](event-forwarding.md), receipt model plus tests |
| Local browser viewer, wire, paste and per-connection teardown | client/server, client/web | [Demo](demo.md), source-reviewed; separate excluded workspace |
| Interactive raw terminal and proposed SDK adapter | examples, docs/examples | [Demo](demo.md), distinguish executable example from sketch |
| Resource quotas, diagnostics and latency | domain/session, application/diagnostics | [Budgets](budgets.md), no measured scale claim |
| Native pin/build, regression/performance/coverage/release tools | scripts/native, scripts/release, scripts/coverage.py, experiments | [Evidence](verification.md), current versus historical scope |

Paths abbreviated above are under crates unless they begin src, client, scripts,
helpers, examples or docs. The dependency direction is domain→standard library,
application→domain, infrastructure→application/domain, facade→composition. External
representations convert at adapters. Immutable values get contracts, not invented
lifecycles. Six bounded models cover selected rules; the other feature rows are
not formal verification by association.

Existing ADRs preserve design history and qualification requirements. Some early
sentences call runtime integration future work; current owner pages describe the
reviewed code checkpoint without retroactively claiming those historical gates
passed. No live-owner restart recovery, dynamic reader handoff, cloud daemon,
general authorization, engine conversion or qualified release is added here.
