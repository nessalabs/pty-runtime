# Projection, parking, checkpoints

**ADRs:** [0003](../adr/0003-session-parking-and-state-transfer.md),
[0005](../adr/0005-domain-boundaries-and-adapters.md)

## Intent

Projected sessions maintain an authoritative native terminal model. Idle models
park to an encrypted disk checkpoint by default (60s without mutation), restore
with READY before history completion, and expose ordered transfer to consumers.
Process ownership continues while parked.

## Code

- `crates/application/src/projection/` — budgets, parking, IO, transfer
- `crates/infrastructure/src/terminal/` — Ghostty adapter
- Checkpoint store / protector adapters under infrastructure

## Verification

See [verification index → projection](../verification/README.md#projection--parking--checkpoints).
Key folders: `loop3/`, `loop4/`, `parser-control-admission/`,
`projection-io-*`, `checkpoint-crash-cleanup.md`.
