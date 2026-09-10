# Reader memory gauges and staging fixture: independent DDD review

Reviewed 2026-09-08 by the independent DDD specialist. Bounded source review of the working tree based on `487ef087efef0337a5adbad58e36806ac3f793ef`; the hashes below identify the actual reviewed content, including untracked sources. Read `AGENTS.md`, `coding_standards.md`, ADR 0005, and the relevant reader/memory/admission requirements in ADR 0002.

## Result

No P1/P2 DDD, dependency-direction, ownership, or conversion blocker found in this selected change. No implementation edits were made. This is a scoped architecture review, not a release qualification or an independent execution of the mechanical gate.

## Findings and evidence

- Application owns measurement contracts and fixed aggregate storage. `diagnostics/reader.rs:1-31` depends only on its application module and standard-library Arc; it retains a diagnostics owner and numeric capacity, not a Vec, descriptor, native pointer, allocator object, or external DTO. `diagnostics/counters.rs:75-139` adds two fixed atomics without labels or per-reader registry entries. The application manifest continues to depend only on domain. A concrete RAII measurement token is appropriate here; no additional service interface is needed for an internal observation operation.
- Infrastructure remains the allocation owner. `process/io.rs:11-29` wraps the actual allocated Vec and registers its capacity after allocation. Field declaration order releases the Vec before the guard. `io.rs:36-48` keeps this owner across read processing, scratch clearing, drained callback and completion notification. A drain callback or reader-done flag can precede destruction; the API correctly promises settled totals rather than equating process/drain completion with immediate scratch release. Keeping completed session handles does not keep this local scratch owner alive.
- Public semantics distinguish observed live allocation from configured session capacity. `diagnostics/reader.rs:4-19` documents opt-in observation, fixed capacity, lifetime responsibility, reset persistence and concurrent approximation. `counters.rs:61-65` excludes stacks and allocator overhead. `counters.rs:118-129` adds/subtracts the same quantity and leaves live gauges intact on reset. Separate relaxed updates are not an atomic paired snapshot; this is consistent with `RuntimeDiagnostics` documentation. The token is an adapter-reporting contract, not a resource-admission permit: callers must obey its stated lifetime/preallocation contract. Saturating arithmetic avoids wrapping but should not be construed as admitting unbounded physical resources.
- Existing admission remains authoritative. `ProcessLimits::validate` bounds read chunks to 1–65,536 bytes and `process/backend.rs:155` validates before spawn. The new guard adds no independent scratch allocation and does not replace process/session limits. Its Arc keeps diagnostics valid through cleanup without extending the scratch lifetime through a session-owned token or introducing a cycle.
- `process_reader_diagnostics.rs:64-155` uses real Unix children and adapter events, checking independently exiting readers, reset preservation, retained completed handles, shutdown, and a caught output panic. Application tests check guard error/unwind destruction. These source assertions address allocation lifetime instead of multiplying configured session counts. They do not prove stack residency, total control-memory compliance, or allocator/kernel accounting.
- Staging-slot fixture configuration uses the existing domain option. `config.rs:33,46-47` parses a bounded 1–1,048,576 value; `population.rs:88-96` applies it only when projection exists, keeping transient cancellation sessions raw. Domain `projection/options.rs:121-122` independently validates the same range. Existing application `projection/coordinator.rs:70-80,114-117` validates and reserves finite queue capacity; `projection/admission.rs:12-36` still applies global and local output quotas separately from control tickets. The fixture override changes neither domain defaults nor those policies.
- `report.rs:3-16,39-46` and `run.rs:8-18` keep JSON formatting at the executable boundary, use explicit null for unavailable baseline/raw configuration, and emit numeric reader gauges. `LOAD.md:29-38,98-113` preserves default comparison evidence and expressly distinguishes reader capacity from stacks, allocator costs and full process memory. No fixture measurement is promoted to a production behavioral or release claim.

## Verification scope and limits

Read-only source and diff inspection completed; no tests, builds, gate, performance workloads or native crash investigation were run by this reviewer. The coordinating agent must retain the required gate and behavioral evidence for these hashes and resolve any other specialist findings before accepting the loop. The native crash is user-deferred and outside this review. This review does not certify full coverage, the 12-hour soak, the <=4 KiB control-state requirement, or complete release readiness.

## Selected source SHA-256

| Source | SHA-256 |
| --- | --- |
| `crates/application/src/diagnostics/counters.rs` | `5a747701539c48f364e9acc44ca8fb232ef6db53e8d224b8cb177a1c09089d29` |
| `crates/application/src/diagnostics/mod.rs` | `a57546539ec6107b5d46ccd9ee8becba7cb771b1558b51d82f7d21142a2dfe94` |
| `crates/application/src/diagnostics/reader.rs` | `e604e817a18e286205eb370d22a6d297e9bdb080d018d1eb0dba2ce22da9ca98` |
| `crates/infrastructure/src/process/io.rs` | `fbe7117ffcd2b7f0b80e05d7658655b6d351d63ee187e9f198fc4b00166af4a7` |
| `crates/infrastructure/tests/process_reader_diagnostics.rs` | `39c9ac0caee9bcf428e0f179c87c136f8904c80a2e07d3c5e4233916370724d0` |
| `examples/release_load_support/config.rs` | `a6b287f8b09d420ad7cfe87eafd3e74a5e665f7c4d9b93e8bd9ad0f3271f401f` |
| `examples/release_load_support/population.rs` | `ecc414ce1a288bb62c8c01ae0893efa079b48ffd50dcebe4391c28c5b3fe879f` |
| `examples/release_load_support/report.rs` | `c7ba2a652e463e8e3a544e61c24a643cb5257ce965aee745c3d24a7dd07bb50a` |
| `examples/release_load_support/run.rs` | `08c12977fef2f3f7fd0df061128a04e55119fac273c94bbded6d64426f6fa097` |
| `scripts/release/LOAD.md` | `fff632ea866770309743e41f8fbb6a66035318e366064e4dfc175d8f814e1041` |

## Rustdoc correction re-review

Independently verified the final source against every hash above: the only changes are Rustdoc backticks around `Vec<u8>` in counters.rs and `Vec<u8>::capacity()` in reader.rs. Reversing those two documentation edits reproduces the original reviewed hashes; all other selected source hashes still match. These corrections prevent generic syntax from being interpreted as HTML and change no ownership or behavior. The DDD conclusion remains unchanged. The coordinating agent reported the failed documentation gate is retained and the full gate will be rerun; this addendum does not claim its result.

These final hashes supersede the two corresponding entries above:

| Source | SHA-256 |
| --- | --- |
| `crates/application/src/diagnostics/counters.rs` | `0175fe984d6f9b91c026e0ef6a1fa3e478f39175c1b1dfd4e51715bb3de15beb` |
| `crates/application/src/diagnostics/reader.rs` | `8c64b1997db65c04f837e8bd6574c2147a92491074538aea01b58c6e189c9933` |
