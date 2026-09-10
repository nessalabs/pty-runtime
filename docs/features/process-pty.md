# Process / PTY ownership

**ADRs:** [0001](../adr/0001-pty-runtime.md), [0005](../adr/0005-domain-boundaries-and-adapters.md)

## Intent

Own real Unix PTY children with dedicated readers by default. Detach and
observer lifecycle never kill or relaunch the process. Cancellation, resize,
write acknowledgement, and bounded replay are first-class.

## Code

- `crates/application/src/runtime/` — session owner, attach, write, cancel
- `crates/infrastructure/src/process/` — Unix backend, spawn, IO, supervision
- Public facade: `pty_runtime::Runtime`

## Verification

See [verification index → process-pty](../verification/README.md#process--pty-ownership).
Key folders: `foundation/`, `loop2/`, `process-pressure-race/`,
`close-completion-race/`, `g1-raw-acceptance` (archived narrative under
`archive/milestones/`).
