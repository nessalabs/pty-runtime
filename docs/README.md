# PTY runtime documentation

## Read in this order

| Document | Purpose |
| --- | --- |
| [usage.md](usage.md) | How to embed the library |
| [features/](features/README.md) | Product behavior by area |
| [adr/](adr/README.md) | Binding design decisions |
| [verification.md](verification.md) | What we checked, in plain English |
| [todo/](todo/README.md) | What is not done, and what was measured and declined |
| [examples/](examples/) | Interactive shell and SDK sketch |
| [experiments/](experiments/) | Early measurements that informed ADRs |

## What we are building

A reusable headless Rust library that owns real Unix PTY processes, accepts byte
input, retains bounded output for reconnect, and optionally maintains terminal
state through libghostty. The host supplies auth and transport. Rendering UI,
daemons, and recovering live processes after owner restart are out of scope.

## Platform

Intended targets are macOS and Linux on arm64 and x86_64. What has actually been
run is narrower, and no target is qualified:

| Target | Evidence level |
| --- | --- |
| macOS arm64 | Exercised — standalone experiment fixtures and native adapter tests have run here. Not qualified |
| Linux x86_64 | Exercised — standalone experiment fixtures have run here. Not qualified |
| macOS x86_64 | **Unqualified** — no executed evidence |
| Linux arm64 | **Unqualified** — no executed evidence |

"Exercised" means something ran and was recorded; it is not a release
qualification pass. The
[ADR 0004 platform table](adr/0004-integration-and-release-qualification.md#platform-qualification)
is the authority, and [verification.md](verification.md) is the plain-English
status.
