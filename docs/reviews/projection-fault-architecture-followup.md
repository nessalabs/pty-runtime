# Projection fault handling: independent architecture follow-up

Reviewed 2026-09-08. No P1/P2 DDD or organization finding in this scope. No
production changes were made. This is a source architecture review, not a new
behavioral acceptance or native rendering verdict.

The exact source manifest is
`docs/verification/loop4/projection-architecture-followup/source.json`, digest
`3e57dcec3b83fb3aff50072b9d47b720c067f919a93605aca1b36de2c3b0699d`.
It includes projection production/tests, domain projection/terminal types,
terminal application ports and both core Cargo manifests. Comparison against
`docs/verification/loop4/organization-source.json` found no changed projection
production file or changed public domain file already listed there. The current
fault/cleanup test files not in the earlier delta manifest were inspected.
Absence from that older delta manifest does not prove a file was newly created.

## Dependency direction and domain ownership

Application still depends only on domain and std; domain has no dependencies.
`crates/domain/src/projection/mod.rs:34` keeps typed boundary errors, and its
`ProjectionStatus` and `ResizeOutcome` at lines 52 and 72 distinguish published
bytes, applied bytes, model controls, history completion and process outcomes.
These are product facts; no errno, native pointer, engine enum, storage record
or scheduler handle enters these types. Opaque checkpoint bytes and the
compatibility token remain explicitly adapter-owned data.

`crates/domain/src/projection/policy.rs:186` validates restoration transitions
and cumulative skipped-page accounting. Application completion code requests the
transition and releases source/memory ownership only after the milestone is
accepted. `RestorationProgress` distinguishes finished source validation from
complete history application; the application does not interpret native pages.
The existing replaceable `ITerminal` port documents serialized ownership,
bounded operations and mutation/error behavior. No additional generic service
or mirrored transport type is needed for these boundaries.

## Fault ownership and organization

`crates/application/src/projection/worker.rs:124` owns orchestration of permanent
failure: it records the domain transition, retains admitted output under its
leases, separates queued request completion from the core lock, and seals the
transfer prefix. Domain policy decides state; application decides which owned
waiters and work remain. This preserves dependency inversion.

`projection/native.rs` separates process-operation completion from model calls
and reports both sides of resize. Adapter errors remain typed at the application
boundary. The term native in the private module does not introduce a native
implementation dependency; calls go through the terminal port.

`projection/teardown.rs:122` owns discarding pending observations after worker
failure. The same module contains the narrowly documented after-shutdown
fallback, where scheduler/executor quiescence is a caller precondition and
uncertain storage remains charged. Keeping this ownership logic beside cleanup
is coherent; it is distinct from routine scheduling in `worker.rs`, external
completion interpretation in `completion.rs`, and source I/O in `io.rs`.

The focused `tests/native_faults.rs`, `tests/io_faults.rs`, and
`tests/shutdown.rs` express observable failure contracts: unsuccessful OS resize
does not advance the model generation; failed reads retain the sole stored
source; lost shutdown completions do not pretend storage was reclaimed. They
use the existing application ports and harness rather than embedding an OS or
engine implementation into core tests. Test bodies were reviewed, not rerun by
this reviewer.

## Checks and limits

Independently ran `scripts/gate.py`'s `architecture()` check: pass. It verifies
workspace dependency direction and the 350 nonblank-line limit across Rust/C/
header inventories. Log: `docs/verification/loop4/projection-architecture-followup/checks.log`.
No speculative restructuring is requested for files that currently satisfy
cohesion and size constraints.

The root-owned full candidate gate was not duplicated. Its result and independent
behavioral/performance/coverage evidence remain required for acceptance. This
review did not run terminal corpus or touch native source. The separately reported
seed 201 native crash and incomplete corpus are unresolved correctness evidence,
not findings this product-architecture review can resolve or waive.
