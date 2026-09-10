# Performance / load

**ADRs:** [0002](../adr/0002-performance-and-stability.md)

## Intent

Finite shared and per-session budgets. Dedicated readers are the initial default
with measured stack cost. Release workloads (throughput, latency, soak, capacity)
are defined in ADR 0002/0004; experiments are not substitutes.

## Code

- `scripts/release/load.py` and load support
- Runtime diagnostics / resource snapshots on `Runtime`

## Verification

See [verification index → performance](../verification/README.md#performance--load).
Key folders: `load-capacity/`, `load-*`, `reader-memory-gauges/`,
`projected-capacity-methodology/`, `release/` (load candidates).
