# Interactive example verification

Requirement: usable raw PTY client with literal argument forwarding, Unicode/ANSI
output, resize propagation, child status, emergency cancellation, bounded host I/O,
restored host settings and reaped workload. Implementation: `examples/interactive.rs`
and `examples/interactive_support/host.rs`; independent findings:
`docs/reviews/interactive-correctness.md`.

Source identity: `source.json` records the dirty working-tree HEAD, scoped SHA-256
fingerprints and the Linux source archive fingerprint. It does not equate the
working tree with HEAD. Date: 2026-09-08.

Commands:

```sh
cargo build --locked --example interactive --no-default-features
python3 scripts/release/interactive_smoke.py
cargo clippy --locked --example interactive --no-default-features -- -D warnings
```

macOS: all seven smoke cases pass (`smoke-macos.jsonl`), scoped Clippy passes
(`clippy-macos.log`). JSONL includes raw end-to-end case durations and stalled-input
cancellation latency for this single-run workload. These elapsed values include
fixture startup/cleanup and are not runtime throughput/latency benchmarks.

Linux: all seven smoke cases pass (`smoke-linux.jsonl`). Bounded stalled-child
cancellation completed in 40.5 ms on Linux and 24.8 ms on macOS.
Linux execution is preserved in `linux-exec.json` with build output and kernel
identity. The isolated raw-only checkout is `/tmp/pty-interactive-review` in the
existing Linux Box; no native engine is required for this example.

Limits: one smoke iteration per platform; no 12-hour soak, release repetition,
full-screen application, renderer compatibility, or output-stall stress qualification.
The bounded nonreading-child test does not expose internal pending-write state.
Full `python3 scripts/gate.py` is deferred to the root's coordinated acceptance
loop because a separate native checkpoint/resize regression is being resolved.
This scoped verification is not repository acceptance.
