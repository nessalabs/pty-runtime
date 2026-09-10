# CI / release qualification

**ADRs:** [0004](../adr/0004-integration-and-release-qualification.md),
budgets in [0002](../adr/0002-performance-and-stability.md)

## Intent

Mechanical gate (`python3 scripts/gate.py`), platform CI, and the adversarial
proof ledger. No ADR milestone is “passed” while required ledger rows lack
scoped executed evidence.

## Code / ops

- `scripts/gate.py`, coverage scripts, release load harness
- Proof ledger: [`../verification/requirements.md`](../verification/requirements.md)

## Verification

See [verification index → CI/release](../verification/README.md#ci--release).
Key folders: `ci-*`, `ci-portability/`, `release/`, `resumed/`,
archived gap audits under `archive/milestones/`.
