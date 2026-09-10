# Guardian / helper image

**ADRs:** [0001](../adr/0001-pty-runtime.md)

## Intent

Each admitted PTY uses packaged helper processes. The helper image is staged so
caller forks never inherit a writable executable (avoids ETXTBSY). Construction
expects a single-threaded / `atfork`-safe caller.

## Code

- `crates/infrastructure/src/process/image.rs`
- `crates/infrastructure/src/process/image_materialize.rs`
- `crates/infrastructure/src/process/guardian.rs`, `spawner.rs`, `supervisor.rs`
- Fixtures: `crates/infrastructure/tests/fixtures/process_image_*`

## Status

See [`../verification.md`](../verification.md).
