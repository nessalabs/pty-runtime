# Coverage source ownership: independent quality review

Reviewer: performance_gate. Reviewed the current
`scripts/guardian/coverage_checks.py`, the dependency binding in
`scripts/guardian/coverage.py`, the five inventory tests, and actual retained
`work/coverage-closure/helper-bound-imports02-evidence`. No P1/P2 finding.

The original inventory assumed every function's primary source belonged to a
local depfile. The new helper tests legitimately instantiate four std TLS
closures and three libc wait-status functions whose primary source belongs to
their definitions. The correction admits bound selected code owners and
retains every original function, region and count, including zero counters.

An installed primary must have a real nonempty file-zero code region. All
nonlocal regions must still be nonempty code regions in the bound source set.
Every local compiled source must still appear as an actual primary owner, and
the measured file set must equal the complete local-plus-external inventory.
Duplicate normalized file summaries remain rejected. Alias metadata now
records whether a region belongs to a primary owner; it does not rewrite the
export. The five retained tests pass and verify unchanged zero records,
unbound-owner rejection, missing-local-owner rejection, metadata-only-owner
rejection and duplicate-summary rejection. Gate unittest discovery includes
them.

Dependency admission follows the actual locked helper Cargo metadata and the
selected library artifact. It verifies the registry archive against the lock
checksum, compares cached Rust source and Cargo.toml bytes against that archive,
checks the full Rust source population, and derives the permitted selected
source subset from the actual artifact's depfile. Only that selected subset is
added to the permitted installed-source owners. Archive, manifest, all package
Rust source, depfile, and source population are checked again after execution.
This is stronger than accepting a registry path prefix, and the added checks
remain separate from the unchanged counter accounting and completeness gate.

Independent read-only verification of the actual bound-imports02 run found:

- All 293 tool, 2,909 installed-source and 59 binary/profile hashes match.
- The selected dependency is libc 0.2.189 with the Cargo registry package ID.
  All 389 Rust source hashes and the archive/manifest hashes match; all 390
  archived Rust-or-Cargo.toml inputs match their cached bytes. The package Rust
  population is exact. All 46 selected depfile sources are in that bound set,
  and its depfile hash matches.
- The actual Cargo JSON selects exactly guardian, startup and fault_recovery
  test executables. Independently reconstructed their dependency source union:
  21 local sources, 28 measured files, seven external source files. All local
  primary owners are retained; there are exactly seven external-primary
  functions, including the two zero-count TLS closures.
- All 71 recorded commands exit zero, including all tests and 23 protocol
  probes. There are 54 raw profiles. Snapshot hashes and the complete copied
  file population match. The physical counter audit retains 296 raw identities,
  270 sparse-merged identities, 26 zero-only omitted identities and 4,020
  counters with exact merge conservation. Export sanity checks all 16,809
  counts, with no negative or saturated values.
- The readiness totals exactly equal the unchanged raw export totals:
  1,872/2,030 lines, 154/155 functions, and 4,735/5,655 regions. Branch coverage
  remains unsupported (0/0), not proven complete.

The complete collector now reaches its strict completeness check and fails
because coverage is below 100%. This is an integrity PASS and readiness FAIL;
no coverage requirement has been waived. The earlier failed source-layout,
primary-owner and missing-cache-metadata attempts remain retained. Fresh full
workspace/mechanical/performance evidence remains separate and pending.
