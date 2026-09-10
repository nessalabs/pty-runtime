# Coverage / native bridge

**ADRs:** [0004](../adr/0004-integration-and-release-qualification.md),
[0005](../adr/0005-domain-boundaries-and-adapters.md)

## Intent

Ghostty is reached only through the native bridge with sized structs, budgets,
and ownership rules. Coverage tooling and boundary tests prove contracts; they
do not alone discharge release rows in the ledger.

## Code

- `scripts/native/` — C bridge, bootstrap, patches
- `crates/infrastructure/src/terminal/` — FFI and projection

## Verification

See [verification index → coverage](../verification/README.md#coverage--native).
Key folders: `native-coverage/`, `native-instrumentation/`, `coverage-*`,
`native-boundary-independent/`.
