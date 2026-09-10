# CI / release qualification

**ADRs:** [0004](../adr/0004-integration-and-release-qualification.md),
[0002](../adr/0002-performance-and-stability.md)

## Intent

The mechanical gate and CI prove the tree still builds and passes fixtures.
Release “done” needs the bigger workloads called out in
[`../verification.md`](../verification.md) and the ADRs.

## Code / ops

- `python3 scripts/gate.py`
- Platform CI
- Plain status: [`../verification.md`](../verification.md)
