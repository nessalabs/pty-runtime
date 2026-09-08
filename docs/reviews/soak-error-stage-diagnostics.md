# Soak error-stage diagnostics

The failed candidate-5 soak retained only `Error: Projection(Worker)` after turn 576. This fixture-only change makes a future returned error identify whether it arose during transient spawn, exit/cancel request, completion wait, forget, or periodic view/checkpoint/resize. It does not assert a cause for the retained failure or fix production behavior.

On error, `soak::diagnose` writes a best-effort `soak_error` line to stderr with phase, turn, original error, and—when a child exists—seed, lifetime, public session status and public projection status. Status lookup failures remain visible as `Err(...)`. A spawn failure has no returned child and explicitly reports unavailable status. The existing JSONL capture records stderr, so the line travels with the failed attempt.

`diagnose` borrows and returns the original result unchanged; stderr write failure is ignored. Periodic projection waits retain their existing RuntimeError conversion. No retry, error suppression, added wait, workload change, success-path log, or success-path status query was added. The existing 15-second deadlines, operation order, assertions, progress placement and 250ms loop delay remain. Non-soak callers of `Harness::finish` pass no diagnostic turn and remain silent. Final teardown and cold-cycle operations are outside this bounded instrumentation change.

Validation performed: `rustfmt --edition 2024 --config skip_children=true` on the two edited files, `git diff --check`, and source review of the result propagation/order. **Compilation, tests and workloads were deliberately not run while the coordinated depth sweep was active.** The coordinating agent owns the subsequent gate.

SHA-256 at handoff:

| File | Frozen `71c1d6f` | Edited |
| --- | --- | --- |
| `examples/release_support/soak.rs` | `717fb12c00bd14745b75868158516dbac60a82db8297b1a7832f9cf717486655` | `7faea6420df8dbd1b04e35a53341efe9aa5d924bb0a024260357f257a835d128` |
| `examples/release_support/mod.rs` | `755e3063107a9166d83d6eaf0a4417e2cb5459374d1933b0830970cc97efa374` | `b5ed1f5f5230a7fce4a41df12967898d73ac9ed92a8ca10cd166b559c454a866` |
