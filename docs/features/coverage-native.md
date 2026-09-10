# Coverage / native bridge

**ADRs:** [0004](../adr/0004-integration-and-release-qualification.md),
[0005](../adr/0005-domain-boundaries-and-adapters.md)

## Intent

Ghostty is only reached through the native bridge, with sized structs and clear
ownership. Coverage tools help find gaps; they do not by themselves mean
“release done.”

## Code

- `scripts/native/`
- `crates/infrastructure/src/terminal/`

## Status

See [`../verification.md`](../verification.md).
