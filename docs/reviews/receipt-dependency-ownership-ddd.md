# Receipt completion and dependency inventory: DDD re-review

Scoped PASS; no new P1/P2 ownership finding. Reviewed current receipt fixture and
collector ownership changes, plus before/after failure evidence and
`helper-bound-imports02-evidence`. No build, test or source mutation was performed;
only this review was written during the coordinator's source freeze.

The receipt fix closes a real fixture ownership mismatch. Closing one descriptor
was insufficient to signal EOF when an unrelated concurrent fork retained a copy.
After write_inner completes and its owned materializer has been reaped (or no
child was created), collect now calls shutdown(Write) on the report socket.
That terminates this socket direction for all descriptor copies without waiting
for unrelated child lifetimes. All legitimate receipt writes precede this point;
the change neither substitutes child completion nor cuts off an active owned
writer. It is test-only, leaving production syscall/reap behavior unchanged.

The regression retains an actual separately forked child while collecting an
already-written F receipt. The child uses only raw poll/read/_exit and stack
storage. A WNOHANG query proves it is still alive after collection, and its owner
releases/reaps it afterward. The old implementation's retained log fails at the
one-second read deadline with WouldBlock; the corrected full infrastructure unit
run passes all 42 tests. This is concrete reproduction and repair, not a longer
sleep or weakened expected receipt. The child guard keeps its actual PID until
reap; no discovered numeric identity is used to signal an unrelated process.

The collector's new ownership relation distinguishes local compiled source from
verified imported code. Cargo metadata names the selected locked registry package;
its Cargo.lock checksum authenticates the cached crate archive. Relevant extracted
build inputs are compared against that archive, Rust-source population membership
is exact, and the actual unique Cargo library artifact supplies the selected
dep-info source subset. Path dependencies, unsupported source kinds, ambiguous
artifacts and unbound dependency sources fail closed. The checker does not decide
ownership from a substring such as `/registry/` or trust any local file merely
because it exists.

Independent read-only checks found 389 Rust source entries in the retained
dependency manifest, a 46-file compiled subset, and no hash mismatches across
source/archive/manifest/dep-info bindings. The reported roughly 390 build-input
population must not be confused with the exact Rust-only entry count. Bound
standard-library macro-generated primary owners and selected dependency primary
owners must carry an actual nonempty CodeRegion. All imported regions and file
summaries remain in the original export; exact source inventory reconciliation
includes them. Counts are not reassigned to local files, dropped, or used to
select ownership. Local compiled owners remain mandatory.

This places package authenticity in the collection/orchestration layer and
region/source reconciliation in the pure checker. No runtime dependency,
production API, domain policy or helper behavior is added. Although this reviewer
authored the older base inventory checker, the new package-selection and imported
primary-owner policy is the coordinator's change and is reviewed here on its
actual data ownership. This is not retrospective self-approval of the older
validator; separate quality/adversarial reviews remain authoritative for it.

All retained child collection commands exit zero. Readiness remains false:
1,872/2,030 lines, 154/155 functions, 4,735/5,655 regions. These are the bounded
helper-bound-imports02 population, not the still-running whole-fault matrix03.
The additional imported owners broaden measured evidence and do not waive the
100% target. Final unchanged-input collection, ordinary-build absence, mechanical
and performance gates remain separate.

Reviewed SHA-256 bindings:
- receipt fixture: `4111e4835a2999fa50e937d37cebbfae135c55929a9ed6d094be4406047683b6`
- collector: `c29a50100f86f8a414a467460cf453da619fb3bc4b688ac7bac72cabdb9d0b42`
- ownership checker: `5c0179b257a5bbb943f80176d971f62538cda1b873a94e8df55a0f0bdfb391e2`
