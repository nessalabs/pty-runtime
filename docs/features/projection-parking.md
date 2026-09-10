# Projection, parking, checkpoints

**ADRs:** [0003](../adr/0003-session-parking-and-state-transfer.md),
[0005](../adr/0005-domain-boundaries-and-adapters.md)

## Intent

Projected sessions keep a real terminal model. Idle models can park to an
encrypted disk checkpoint (default: 60s without changes), restore carefully, and
hand state to consumers in order. The process keeps running while parked.

## Behavior

- Private namespace + runtime-owned encryption key; trusted parent and size ceiling
  via `StorageOptions`.
- Normal teardown removes owned objects. Crash cleanup uses directory locks (not
  PIDs) and finite budgets; incomplete cleanup can block new temporary stores.
- No recursive delete, no reading ciphertext to decide what to remove, no auto
  cleanup of old dirs outside the arena.
- Restarting the owner process does **not** bring back live sessions (ADR 0003).
- More embedding detail: [`../usage.md`](../usage.md).

## Code

- `crates/application/src/projection/`
- `crates/infrastructure/src/terminal/`
- Checkpoint adapters + `checkpoint_*` tests under infrastructure

## Status

See [`../verification.md`](../verification.md).
