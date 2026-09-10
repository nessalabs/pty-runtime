# Performance / load

**ADRs:** [0002](../adr/0002-performance-and-stability.md)

## Intent

Shared and per-session budgets stay finite. Release load (throughput, latency,
soak, capacity) is defined in the ADRs. Experiments are not a substitute.

## Code

- `scripts/release/load.py` and related support
- Runtime resource snapshots on `Runtime`

## Status

See [`../verification.md`](../verification.md) — full release load is still open.
Keep bulky run output outside git.
