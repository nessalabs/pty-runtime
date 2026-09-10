# Scrollback / history

**ADRs:** [0006](../adr/0006-scrollback-projection.md),
[0003](../adr/0003-session-parking-and-state-transfer.md)

## Intent

History is a **read-only range** of rows in the current window. Do not scroll a
shared viewport to serve readers. Indexes move when old rows are evicted. Live
history is byte-budgeted (`history_bytes`); infinite cold storage is future work
(demo default stays a finite hot window, e.g. 16 MiB).

## Code

- `crates/domain/src/terminal/view.rs` (`TerminalHistory`)
- History projection in infrastructure
- Demo: wheel → `history` messages in `client/`

## Status

See [`../verification.md`](../verification.md).
