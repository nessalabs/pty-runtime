# Guardian / helper image

**ADRs:** [0001](../adr/0001-pty-runtime.md) (process ownership / helpers)

## Intent

Each admitted PTY uses packaged helper processes. The guardian image is
materialized privately so caller forks never inherit a writable executable fd
(ETXTBSY). Construction has a hard single-threaded/`atfork` precondition.

## Code

- `crates/infrastructure/src/process/image.rs` — staging owner
- `crates/infrastructure/src/process/image_materialize.rs` — fork-isolated writer
- `crates/infrastructure/src/process/guardian.rs`, `spawner.rs`, `supervisor.rs`

## Verification

See [verification index → guardian](../verification/README.md#guardian--helper-image).
Key folders: `guardian/`, `bundled-helper-image/`, `helper-image-fork-contract/`,
`image-fork/`, `candidate6-spawn-*`.
