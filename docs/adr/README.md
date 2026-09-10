# Architecture decision records

Binding decisions for the library. Feature pages under [`../features/`](../features/README.md)
point here; they do not replace ADRs.

| ADR | Topic | Feature buckets |
| --- | --- | --- |
| [0001](0001-pty-runtime.md) | Scope, API ownership, lifecycle, Ghostty pin, delivery | process-pty, guardian-helper, client-demo |
| [0002](0002-performance-and-stability.md) | Resource budgets, latency targets, release workloads | performance-load, ci-release |
| [0003](0003-session-parking-and-state-transfer.md) | Checkpoints, parking, restoration, transfer | projection-parking |
| [0004](0004-integration-and-release-qualification.md) | Concurrency, cancellation, storage races, pass criteria | ci-release, coverage-native |
| [0005](0005-domain-boundaries-and-adapters.md) | Dependency direction, ports, adapters | process-pty, projection-parking, coverage-native |
| [0006](0006-scrollback-projection.md) | Read-only scrollback range queries (not viewport scroll) | scrollback-history, client-demo |

Numeric targets in ADRs remain proposals until the linked verification rows pass.
