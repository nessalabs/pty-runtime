# Foundation review loop

Status: implementation foundation only; G1–G4 remain pending.

The private `nessalabs/pty-runtime` repository contains the four-layer Cargo
workspace, mandatory coding standards, mechanical gate, specialist review records,
and the initial domain replay/identity primitives. No process adapter or terminal
engine is implemented by this loop.

## Evidence

The final gate output and source manifest for this loop are stored under
`foundation/`. The manifest identifies tested implementation inputs without
requiring a self-referential commit hash. A later Git revision changing only this
report does not establish new executable evidence.

- Mechanical gate: dependency directions and identity, all source size checks,
  independent core tests, Rustfmt, Clippy with warnings denied, workspace tests
  with/without features, Rustdoc, experiment validator tests, and gate-negative tests.
- MSRV: workspace tests executed with Rust 1.85.0 on macOS arm64.
- Domain evidence: explicit replay gaps, byte limits, foreign/future cursor
  rejection, zero retention, read retry, checked offset overflow, and reference
  suffix comparison. Global reservation and runtime lifetime issuance remain pending.
- GitHub repository privacy verified with `gh repo view --json visibility,url`.
  Configured GitHub workflows are separate from executed local evidence.

## Adversarial findings and disposition

- DDD review found the architecture gate accepted missing/renamed protected
  crates and dependencies impersonating the domain package. Required package
  identity, paths, workspace membership and dependency paths now fail closed;
  negative gate tests cover the challenges.
- Independent core test runs were added to avoid relying solely on workspace
  feature unification.
- Correctness/DDD review found a short session ID could retain an arbitrarily
  large String capacity. Compact boxed-string ownership removes excess capacity.
- Organization review verified focused module boundaries and the 350-nonblank-line
  production source gate. The historical experiment harness has separate size
  concerns; its raw protocol is unchanged by this foundation.

See [DDD review](../reviews/initial-ddd-review.md),
[organization review](../reviews/initial-organization-review.md), and
[requirements ledger](requirements.md). Integrated resource, process, native,
parking and soak claims require their own subsequent proof.
