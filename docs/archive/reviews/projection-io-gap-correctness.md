# Projection I/O gap tests: independent static correctness review

Reviewed 2026-09-08. Scope: new io_parking_faults/io_pressure tests, terminal fixture seams and test module registration, checked against the production I/O admission, worker, leases and domain retry policy. No builds/tests/workloads or production edits performed while the macOS matrix runs.

## Result

No P1/P2 defect found by static review. The assertions test useful rejection, source-retention and recovery contracts. Compilation and execution are **unverified** by this review; the tests must pass before accepting their coverage or behavioral evidence.

## Evidence

- The terminal checkpoint seam defaults to unchanged descriptor/content and requests at least the original byte length. Wrong control generation and excess Vec capacity target distinct checks in start_park. Tests require the original resident owner, unchanged processed/published byte position, zero store commits, released temporary leases, no terminal failure and explicit parking failure. Zero protector calls proves rejection precedes provider dispatch.
- FaultProtector separately injects authentication error, wrong key, wrong descriptor, empty ciphertext and ciphertext exceeding the promised bound. It keeps protection/open inside the existing replaceable application port. The tests check no committed source and retain the original model, then remove the fault and recover the same bytes through a real application checkpoint operation. These are application test doubles, not encryption/native validation.
- Retry timing is consistent: the fixture clock measures seconds; default park_after is 60 seconds and retry_after is 5 seconds. Failure at 60 followed by a pump at 64 must not call protect again; 65 permits retry. The helper verifies one successful commit, exact saved bytes/control boundary, one eventual delete and release of all listed resource quotas.
- IoMemory takes plaintext then protected leases from the same quota. The 2,088 total with a held 1,064 leaves 1,024: plaintext acquisition succeeds, protected acquisition fails, and the partial plaintext lease must drop. The transfer test checks Capacity, no submitted read/restore, the original persisted source, only the externally held 1,064 remaining, zero observer/request usage, and successful later retrieval while still parked.
- The restore loop covers both I/O memory pressure and resident quota pressure. It checks pending observation, parked residency, processed=6/published=11, five staged bytes and no premature restore. Dropping the held lease must yield exact beforeafter content, one Restore and one Feed(after), followed by source deletion and released temporary/staging memory. This distinguishes retryable capacity pressure from lost output or duplicate restoration.
- The garbage-delete case first invalidates an in-flight parking commit by staging more output, so the saved reference becomes garbage while the live model remains authoritative. Blocking-executor rejection must retain that exact reference and its storage charge without incrementing store deletes; further output still processes. After submission resumes, deletion occurs once and a fresh checkpoint contains abc. This exercises the failed-submit requeue path rather than fabricating a failed store response.
- Static names/types/access appear consistent with sibling test support: CheckpointRef is copied, fixture traits are imported, fault state uses existing Arc/Mutex/atomic patterns, and manual stepping uses the existing bounded Harness. New fields are default-initialized; no production seam was introduced. Test module registration is present.

## Limits

No claim of measured coverage increase, compiler success, Linux/macOS pass, native correctness, scheduling timing on real infrastructure, or release readiness follows from this static review. Remaining untested I/O branches must stay in the coverage inventory. The terminal Vec construction may choose different spare capacity than Clone previously did; content and minimum capacity remain equivalent for the fixture, and existing fixture tests need the normal suite run.

## Source SHA-256

| Source | SHA-256 |
| --- | --- |
| `crates/application/src/projection/tests/io_parking_faults.rs` | `ed6b5bda904faad26e230b2cdcf19efa8f8ad0eb5efe8e6a150ac4f32d68808e` |
| `crates/application/src/projection/tests/io_pressure.rs` | `f75329484869756833376d93b017e16c29409ca34032f3ae6178c52dcc905a30` |
| `crates/application/src/projection/tests/terminal.rs` | `39afa1777b8f4db5322c8f0e4138d6369020904a249ec1fb1a8564c5e0726ae0` |
| `crates/application/src/projection/tests/mod.rs` | `618d96b32a1116719cec321ef8edff611a6b35cfdab4e3dc224a2be81d8dc5cd` |
| `crates/application/src/projection/io.rs` | `eeb7106d8656466199a3e63269443cfdd2d1b42a45ba91c67c6f0b30b8a66823` |
| `crates/application/src/projection/budgets.rs` | `261782f9950f20a46fabf7c073156cf828cdb83ac98114cc44f6b78558cd3633` |
| `crates/application/src/projection/tests/support.rs` | `0801fda0c5f22fcf1770b7c4283d0688b964ab505aed8f91e3d566e3f170d9a9` |
