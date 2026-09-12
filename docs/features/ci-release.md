# CI / release qualification

**ADRs:** [0004](../adr/0004-integration-and-release-qualification.md),
[0002](../adr/0002-performance-and-stability.md)

## Intent

The mechanical gate and CI prove the tree still builds and passes fixtures.
Release “done” needs the bigger workloads called out in
[`../verification.md`](../verification.md) and the ADRs.

## Code / ops

- `python3 scripts/gate.py`
- CI: `.github/workflows/runtime.yml` and `.github/workflows/experiments.yml`,
  on the `ubuntu-24.04` and `macos-15` GitHub-hosted images — Linux x86_64 and
  macOS arm64. No workflow selects a macOS x86_64 or Linux arm64 runner.
- Plain status: [`../verification.md`](../verification.md)

## CI is not platform qualification

Running the gate on a runner shows the tree builds and its fixtures pass there.
It is not a qualification pass for that target, and a configured matrix entry is
not evidence at all. Per-run CI results are not retained in this repository.
Consequently CI covers two of the four intended targets and qualifies none; the
[ADR 0004 platform table](../adr/0004-integration-and-release-qualification.md#platform-qualification)
holds the per-target evidence, and macOS x86_64 and Linux arm64 are unqualified.
