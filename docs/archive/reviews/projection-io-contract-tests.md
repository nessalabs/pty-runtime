# Projection I/O contract tests: independent authorship record

Five focused application tests (11 table-driven subcases) now exercise the four justified gaps from `coverage-candidate5-independent.md`. This is a source-authorship record only: **not compiled or executed**. Runtime testing is deferred to the coordinating agent after the active macOS matrix or in its separate Linux diagnostic clone. No production code or frozen candidate-6 file was changed, and no native engine/crash investigation is included.

The existing `io_faults`, `cleanup`, `parking`, `pressure`, provider and terminal fixtures were inspected first. Their current assertions cover malformed restore reads, executor refusal during parking/transfer, stale accepted cleanup, permanent delete failure and checkpoint pin pressure. They do not cover the new pre-publication contract violations, pre-I/O shared quota denial, or refused garbage-delete submission. The tests therefore assert observable state/ownership and recovery, rather than merely driving uncovered lines.

## Cases and intended proof

- `io_parking_faults::parking_rejects_wrong_descriptor_or_excess_capacity_before_protection`: two provider violations return the distinct InvalidConfiguration/Capacity parking failures; no protection/storage work occurs, the live model and applied prefix survive, attempted checkpoint/storage leases are released, and a later valid park produces exactly the original bytes and descriptor.
- `io_parking_faults::failed_or_malformed_protection_never_publishes_and_allows_a_valid_retry`: five outcomes (provider error, wrong key, wrong descriptor, empty ciphertext, excess ciphertext) must not call commit or create an uncertain-storage charge. The live model and typed parking failure survive, retry does not run before the configured deadline, and a valid retry preserves the original checkpoint. Closing after recovery releases all tracked projection budgets.
- `io_pressure::transfer_memory_pressure_releases_partial_admission_and_preserves_saved_source`: a real shared lease leaves enough budget for plaintext but not the protection buffer, forcing partial-admission rollback. The transfer fails Capacity without submitting a read, leaking observation/request permits or changing the saved reference. Releasing the competing lease allows a fresh checkpoint without native restoration.
- `io_pressure::restore_memory_or_resident_pressure_keeps_ordered_output_until_capacity_returns`: two shared-budget cases leave the view pending and published output unapplied while the saved source stays charged. No read/restore is submitted before capacity exists; partial checkpoint leases are rolled back. Capacity release restores once, applies the exact queued suffix once, yields `beforeafter`, deletes the superseded source and allows complete cleanup.
- `io_pressure::rejected_garbage_delete_retains_the_same_source_and_live_output_keeps_progressing`: an output mutation invalidates a pending park commit. Executor refusal of its stale-source deletion preserves the same reference and storage charge while live output continues. Later admission deletes once, releases the charge and preserves the complete `abc` state.

The only common fixture extension is two default-off checkpoint fault controls in `tests/terminal.rs`. Its checkpoint bytes still represent the same logical terminal data; the excess-capacity case deliberately grows allocation capacity without inventing extra payload. Protection fault behavior stays inside the new test module, and no common Harness/provider interfaces were changed. The shared quota competitors are real RAII leases, not altered budgets or capacity estimates.

All affected/new test sources remain below 350 nonblank lines. `docs/verification/projection-io-contract-tests/source.json` records the committed base, exact source hashes, counts and explicit execution status, including unchanged production I/O/budget hashes. Formatting used rustfmt only. No passing result, RED/GREEN sequence, coverage improvement or release milestone is claimed until the coordinator executes and independently reviews these tests.

Suggested scoped execution by the coordinator (not run by the author):

```text
cargo test --locked -p pty-runtime-application --lib projection::tests::io_parking_faults
cargo test --locked -p pty-runtime-application --lib projection::tests::io_pressure
```
