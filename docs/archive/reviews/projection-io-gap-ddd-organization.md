# Projection I/O gap tests: DDD and organization review

**No P1/P2 finding in the reviewed test changes.** The new cases belong at the application projection boundary and exercise meaningful ownership outcomes. This is a static-only review: no compilation, tests, mutation testing, coverage run, native/crash investigation, or workload was performed while the Mac matrix was live. Execution/coverage claims require the coordinating agent's gate.

## Reviewed identity

| File | Lines | SHA-256 |
| --- | ---: | --- |
| `crates/application/src/projection/tests/io_parking_faults.rs` | 195 | `ed6b5bda904faad26e230b2cdcf19efa8f8ad0eb5efe8e6a150ac4f32d68808e` |
| `crates/application/src/projection/tests/io_pressure.rs` | 221 | `f75329484869756833376d93b017e16c29409ca34032f3ae6178c52dcc905a30` |
| `crates/application/src/projection/tests/terminal.rs` | 227 | `39afa1777b8f4db5322c8f0e4138d6369020904a249ec1fb1a8564c5e0726ae0` |
| `crates/application/src/projection/tests/mod.rs` | 24 | `618d96b32a1116719cec321ef8edff611a6b35cfdab4e3dc224a2be81d8dc5cd` |

Supporting source read: application `projection/{io,completion,budgets,worker,snapshot,transfer}.rs` and existing `tests/{support,providers}.rs`. No production diff is part of this review.

## Ownership and budget semantics

- The terminal fake produces a wrong checkpoint descriptor or excess **Vec capacity**, so the parking test targets the application's validation boundary before protection. It checks zero protector calls/commits, retained live state and unchanged byte positions, released temporary reservations, and successful later byte-preserving checkpoint/cleanup. It is stronger than merely expecting an error variant.
- `FaultProtector` stays local to the parking-fault module and implements the application protector port. Wrong key, descriptor, empty/oversized ciphertext and a typed protection error are injected separately. The wrapper preserves the existing size-limit contract at coordinator creation: its delegated bound matches the original protector. Replacing the private service in this deterministic harness is test setup, not a new production configuration path or domain dependency.
- Parking rejection remains distinct from terminal failure: the tests require `Resident`, no overall failure, the expected `parking_failure`, and retained native reservation. The retry check at clock 64 and success at 65 exercises the existing retry policy without sleeping. Valid retry proves the original `before` bytes remain recoverable.
- The `2088` checkpoint-memory limit is the fixture's 1024-byte plaintext reservation plus its 1064-byte protected bound. Holding 1064 allows the first 1024-byte acquisition and forces the protected acquisition to fail. Assertions that usage returns to the held 1064 therefore test partial-admission rollback. In the resident-pressure variant, zero remaining checkpoint usage also checks that successful temporary I/O reservations are released when resident admission fails.
- `stored_bytes.used == 1064` denotes the conservative disk reservation, **not** the actual encoded length of `before`. The test correctly preserves that reservation and one stored identity until deletion. Source identity equality, no delete calls and unchanged store membership distinguish “retry later” from losing the sole checkpoint source.
- Ordered restore pressure retains five staged `after` bytes, one request and a pending view while published/processed offsets differ. After release, the byte result `beforeafter`, one restore, one feed of `after`, restored position and cleanup checks substantiate no loss/reordering/duplicate application in this scenario.
- Rejected garbage deletion is rejection by the blocking executor before provider work runs. The test verifies the same source stays retained, live `abc` output progresses, then exactly one provider deletion succeeds after admission returns. It does not confuse this with a provider delete failure or permission to drop uncertain storage.

## Organization and test strength

The files separate two cohesive concerns: validation before relinquishing live state, and admission pressure while retaining saved/garbage state. Their helpers express repeated postconditions rather than duplicating coordinator algorithms. The two terminal-probe fields are default-disabled, test-only boundary hooks; the default checkpoint still carries the same bytes/descriptor. The capacity hook guarantees at least the requested over-limit capacity without assuming allocator size-class behavior.

Module registration is confined to the existing application test module. No infrastructure dependency, OS thread/process manipulation, public configuration knob or generic production fault framework was introduced. The 195/221/227-line fixtures remain cohesive and below the repository's 350-line production-source guideline even though they are test code. A new helper module or generalized injection framework is unnecessary for this scope.

Private `Lease` use deliberately represents an already-held shared reservation. It isolates admission/release semantics without creating a second coordinator. Accordingly, these are deterministic single-owner pressure tests; they do not prove real cross-session fairness, wake delivery, concurrent scheduler behavior, cryptographic integrity, provider durability or native terminal equivalence. The fake protector explicitly supplies orchestration evidence only.

No weak/misleading assertion rises to P1/P2. One optional extension would retry `begin_transfer` itself after the transfer-pressure failure, in addition to the current fresh-checkpoint read, to demonstrate recovery of the same compound API. The present test title claims partial-admission release and source preservation, which its counters and byte readback support; it does not claim a successful transfer retry. Hard-coded 1024/1064/2088 fixture quantities are consistent with `options()` and the fake protector, but should change together if those fixture contracts change.

Static disposition: no DDD/organization blocker for the exact hashes above. Preserve the bounded scope and run the ordinary test/format gate when the coordinating agent permits it; this review is not an execution pass or a claim that all projection I/O paths are covered.
