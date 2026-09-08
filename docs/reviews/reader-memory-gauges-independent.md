# Independent correctness review: reader allocation gauges

No P1/P2 finding in the changed reader-allocation diagnostics. Reviewed application diagnostics (`reader.rs`, `counters.rs`, `mod.rs`), infrastructure reader ownership and registration/join paths, real-reader acceptance tests, and release fixture checkpoint/aggregate reporting. Exact reviewed source hashes, platform, commands and results are in `docs/verification/reader-memory-gauges-independent/source.json`; source remained unchanged during independent tests.

## Ownership, reset and concurrency

Infrastructure allocates the same fixed scratch Vec as before, then creates the optional observation guard using its actual `capacity()`. `ReaderScratch` declares the Vec before the guard, so field destruction releases the buffer before decrementing the observation. The guard retains only the diagnostics Arc and recorded byte count; it neither owns OS state nor changes admission/read behavior. No per-read/per-byte gauge work was introduced. With diagnostics absent, scratch remains allocated and no gauge is registered.

The reader's existing catch-unwind path still converts an output callback panic to `DrainOutcome::Failed(Internal)`, clears scratch, catches a drain-callback unwind and exits through the scratch owner. Ordinary error/EOF and thread unwinding release the same gauge. Allocation occurs before guard construction, so failed allocation cannot leave a phantom registration; the allocation itself remains outside the pre-existing catch-unwind boundary. Join paths wait for reader return. Retaining session/event handles does not retain the scratch owner.

Live-reader and scratch atomics are separate from cumulative counters. `reset_quiescent` resets cumulative/histogram measurements while preserving current gauges; later guard drops therefore subtract still-accounted ownership. Relaxed operations and separate count/byte updates mean a concurrent snapshot can combine nearby states. This is documented as approximate; the fields are not a transactional pair or a reservation mechanism. Actual byte totals are Vec capacity, excluding allocator bookkeeping, usable-size rounding, stacks, PTY/kernel/helper costs and unrelated fixture allocations. They do not establish the 4 KiB control-state target.

## Readiness and reporting interpretation

The fixture's control-socket ready handshake is not a barrier proving the dedicated PTY reader has allocated scratch. Similarly, `events.drained` and `reader_done` occur shortly before `ReaderScratch` leaves scope. A concurrent ready/completion snapshot can therefore temporarily differ from session count or show a reader awaiting final destruction. Reporting preserves actual observed values without asserting inferred reader counts; joined/quiescent totals are the exact cleanup evidence. Do not interpret a ready checkpoint as guaranteed complete reader inventory or a drain callback alone as zero-memory proof.

The baseline without a diagnostics object emits JSON null. Runtime/ready/measurement/closed checkpoints and aggregate records use the shared diagnostics object and emit numeric fields. No multiplication by configured reader size/session count replaces observation. The fixture retains its measurement/reset order and existing census handshake. These are bounded snapshots, not synchronization changes.

## Independent verification

Using `CARGO_TARGET_DIR=work/load-capacity-test-target`, independently executed:

- `cargo test --locked -p pty-runtime-application diagnostics::reader`: both guard tests pass. They cover independent buffers, reset preserving live gauges while clearing counters/histograms, ordinary error return and unwinding cleanup.
- `cargo test --locked -p pty-runtime-infrastructure --no-default-features --test process_reader_diagnostics`: both real-reader tests pass. Actual PTY output readiness establishes live allocations; independent 4093/8191-byte buffers survive reset, normal EOF releases only one despite retained handles, shutdown releases the other, and a caught output callback panic preserves Failed(Internal) and releases memory after join.

Raw logs are `guard.log` and `real-readers.log`. The author's retained pre-hook red demonstrates missing real-reader instrumentation, not an underlying runtime memory leak; this distinction is correct. Tests prove these focused ownership paths, not complete fault-path coverage or quantitative performance acceptance.

No source changes, heavy workloads, native crash work or Linux operations were performed by this reviewer. Full gate, fresh coverage and release accounting remain coordinating acceptance work.

## Rustdoc-only final hash addendum

The gate reported invalid HTML for bare generic Rust type text. Root added code backticks around `Vec<u8>` in counters.rs and `Vec<u8>::capacity()` in reader.rs. Independently verified that removing only those backticks reproduces the exact prior reviewed file hashes for both files; there are no code/behavior changes. Final identities are in `docs/verification/reader-memory-gauges-independent/rustdoc-final-hashes.json`. The correctness verdict is unchanged. No tests were repeated for this documentation-only correction; the coordinating task retains the failed gate and owns the subsequent complete gate run.
