# Scrollback / history

**ADRs:** [0006](../adr/0006-scrollback-projection.md), restore rules in
[0003](../adr/0003-session-parking-and-state-transfer.md)

## Intent

Expose retained history as a **read-only range query** (absolute rows within the
current engine window). Do not scroll a shared viewport to serve readers.
Indices are window-relative and shift under eviction. Live scrollback is
byte-budgeted (`history_bytes`); cold disk paging for infinite history is a
future extension, not the current demo default (16 MiB hot window).

## Code

- Domain: `crates/domain/src/terminal/view.rs` (`TerminalHistory`)
- Infrastructure: `rt_rows` / history projection
- Demo client: wheel → `history` WebSocket messages (`client/`)

## Verification

See [verification index → scrollback](../verification/README.md#scrollback--history).
Key folders: `scrollback/`, `page-admission/`, `page-capacity/`,
`continuation-c1/`, `decode-budget/`, `mouse-modes/`, `packed-pages/`.
