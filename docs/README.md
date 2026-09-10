# PTY runtime documentation

Implementation and release qualification are in progress. Standalone experiments
and one-off review notes are **not** release proof. The proof ledger is
[`verification/requirements.md`](verification/requirements.md).

## Read in this order

| Document | Purpose |
| --- | --- |
| [usage.md](usage.md) | Embed the library: ownership, raw vs projected, bounds |
| [adr/](adr/README.md) | Binding architecture decisions (0001–0006) |
| [features/](features/README.md) | Current feature map and where code/evidence live |
| [verification/](verification/README.md) | Proof ledger + evidence indexed by feature |
| [examples/](examples/) | Interactive shell and SDK sketch |
| [experiments/](experiments/) | Pre-implementation measurement that informed ADRs |

Archived specialist reviews and milestone narratives live under
[`archive/`](archive/README.md). Prefer feature pages and the ledger over those.

## What we are building

A reusable headless Rust library that owns real Unix PTY processes, accepts byte
input, retains bounded output for reconnect, and optionally maintains terminal
state through libghostty. The host supplies auth and transport. Rendering UI,
daemons, and recovering live processes after owner restart are out of scope.

## Platform

Targets: macOS and Linux, arm64 and x86_64. Executed coverage is recorded under
`verification/`; do not treat “target” as “proven.”
