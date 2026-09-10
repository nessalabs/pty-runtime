# PTY runtime documentation

## Read in this order

| Document | Purpose |
| --- | --- |
| [usage.md](usage.md) | How to embed the library |
| [features/](features/README.md) | Product behavior by area |
| [adr/](adr/README.md) | Binding design decisions |
| [verification.md](verification.md) | What we checked, in plain English |
| [examples/](examples/) | Interactive shell and SDK sketch |
| [experiments/](experiments/) | Early measurements that informed ADRs |

## What we are building

A reusable headless Rust library that owns real Unix PTY processes, accepts byte
input, retains bounded output for reconnect, and optionally maintains terminal
state through libghostty. The host supplies auth and transport. Rendering UI,
daemons, and recovering live processes after owner restart are out of scope.

## Platform

Targets: macOS and Linux, arm64 and x86_64. See [verification.md](verification.md)
for what has actually been checked.
