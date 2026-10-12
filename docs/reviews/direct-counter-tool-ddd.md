# Direct-counter measurement tool: independent DDD review

## Current decision

**Scoped frozen V9 DDD/source-ownership PASS.** The two demonstrated V8 extended
calibration blockers are resolved in the reviewed code/evidence; no new P1/P2
DDD or source-ownership finding. Root's remaining old-five/function-entry and
canonical-matrix validation is still pending this review. No integrated gate,
full-source measurement or 100% coverage claim follows.

## Frozen v2 structural review

**Scoped dependency/ownership design PASS; measurement acceptance pending.** No
P1/P2 dependency-direction or ownership defect found in the frozen v2 source.
This does not close the demonstrated v1 continuation-count P2: unchanged oracle
and source-inventory evidence must establish that separately. No build, test,
benchmark or production edit was performed by this reviewer.

Reviewed `work/counter-integrity/direct-counters-v2/driver.rs` SHA-256
`3a296a79c27ee6ef7a0a501ebf9965fe40a6ac30d7a754dc689541545e25e47c`,
`coverage_entry.rs` SHA-256
`afc66afc217d71409869fb3bdb51cbd9adf7823eadbb3154142e5e31804a080f`,
and `build.py` SHA-256
`3fbd184f312477ab550b9f74fb0b9a980f50c106a4fdac3f9aeb396d7fbade63`.
All recorded input/compiler/library/binary/generated-source hashes in
`build-metadata.json` matched files when independently checked. Binary SHA-256:
`d4272f107e1ab949812311001b1a4ab4ff625f70b6bb77e32e1ea6ab7f6f8939`.
Read the v1 quality/adversarial findings, exact installed compiler query and
`mir_promoted`/`InstrumentCoverage` source, generated mapping/graph extraction,
and actual replacement provider, not only the author's description.

## Information owners and boundaries

The runtime's domain/application/infrastructure dependencies are untouched.
Compiler-private crates, dynamic rustc-driver linking and rustc-dev source are
measurement-tool dependencies, currently in ignored diagnostic work. Eventual
integration must preserve this separation under scripts/tooling; nothing here
justifies adding rustc-private or bootstrap requirements to runtime crates.

The original compiler query still owns eligibility, promotion and all executable
MIR. The adapter calls it first and changes only bodies already carrying
`function_coverage_info`. It removes only virtual-counter statements, retaining
span and branch markers and `coverage_info_hi`. Inspection of the exact stock
pipeline confirms coverage instrumentation is the last transforming pass in
`mir_promoted`; its mapping extraction borrows rather than consumes the retained
metadata. Promoted fragments return untouched. No executable statement,
terminator or production source behavior is rewritten by the adapter.

Pinned stock modules own HIR identity, span extraction and source mappings. The
custom graph change prevents a coverage block from crossing call and other
potentially interrupting terminators. The custom entry point regenerates graph,
node-flow data, mappings and virtual counters together, preserving their common
block-number ownership; reusing old mappings with new block IDs would be invalid,
and this implementation does not do that. Source hash equality is asserted
before replacement. Failure to extract any mappings aborts rather than silently
dropping a previously instrumented function.

The final query retains stock surviving-node and inline-scope treatment, assigns
physical counters to mapped surviving nodes, and preserves zero for removed
nodes. Atomic increments are mandatory. No source filename, line number, runtime
error path or particular oracle is special-cased. Source mapping regeneration may
split or coalesce regions: function-source hash identity alone does not prove
preservation of their source footprint. That is an explicit pending measurement
contract, requiring count-independent function/file/kind/position comparison.

## Compiler identity and limits

The builder requires the full compiler commit, checks every copied upstream
module hash, asserts a unique graph-patch preimage and records generated source,
compiler, linked library and binary identity. Linked-driver version is checked
in process; the built driver's complete version output is compared with the
selected compiler. Unsupported source/toolchain combinations fail instead of
falling back to stock counts. The wrapper is deliberately limited to this pinned
Rust 1.98.1 macOS dynamic-driver build. A compiler upgrade needs a new reviewed
adapter and calibration, not a relaxed version check.

Preserving stock eligibility does not mean every production source construct is
instrumented: stock exclusions such as compile-time bodies and derived items
remain a separate inventory obligation. They must be reported as unmeasured
where applicable, never silently treated as satisfying the user's no-exclusions
100% target. The same applies to native, guardian topology and platform coverage.

## Required evidence re-review

Pending: unchanged normal/panic/process-exit oracle results; count-independent
mapping-footprint audit on actual selected runtime functions; inline, optimized
and branch fixtures; helper fork/exit/kill profile ownership; full feature/source
inventory and final gate/performance evidence. The frozen-source design pass is
not authoritative coverage acceptance or a runtime readiness claim. The retained
v1 failed calibration remains valid evidence and must not be overwritten.


## Evidence re-review: P2 mapping footprint loss

The previously pending mapping contract is now a demonstrated blocker.
`work/counter-integrity/direct-counters-v2/footprint-comparison.json` compares
interval unions by function, source filename and region kind, independently of
counts. The reviewer read its comparison implementation and inspected actual
stock/v2 export records at a concrete lost source token.

For `reader-panic` and `reader-normal`, all 1,427 function records remain, but
741 function/file/kind groups lose mapped source positions. Reported source-line
denominator falls from 8,978 to 8,516. For `context-panic`, all 1,133 function
records remain, but 662 groups lose positions and the line denominator falls
from 9,702 to 9,277. Delayed EOF loses positions in 742 groups, with lines
8,972 to 8,516. These are source-inventory losses, not changes to covered counts.

Concrete independently checked evidence: in `crates/infrastructure/src/scheduling/pool.rs:121`,
`StdWorkScheduler::shutdown` assigns `state.closed = true`. Stock reader-panic
export contains code region `[121,13,121,32,0,0,0,0]`, including the assignment.
The v2 export has **no code region** covering line 121 column 27. The footprint
audit identifies missing columns 25–32 (` = true`). Other losses include field
access `.state` and operators. They cannot be dismissed as whitespace or solely
as span splitting with an equivalent source union. More total regions in v2
does not compensate for removal of different production source positions.

The corresponding information-loss boundary is
`generated/coverage/spans.rs:185` (`remove_unwanted_overlapping_spans`). Its stock
rule drops a pending outer span when an overlapping inner span has a different
coverage-block owner. Splitting more graph blocks changes that predicate;
reusing the stock refiner is therefore not itself proof of preserving the stock
source footprint. The tool currently replaces previous mappings without a
preservation guard, so the failure reaches a successful exported measurement.

Required fix and re-review: retain meaningful source positions with correct
counter ownership, fail closed if the preservation contract is violated, and
rerun the same independent side-effect oracles and count-independent footprint
audit. Do not restore positions using fabricated positive counts, discard the
stock baseline, exclude these paths, or treat function-hash equality as a
substitute. The four actual runtime tests passing and selected false/true EOF
counts improving do not close this P2. No proposed follow-up fix has been
reviewed here, and no 100% or runtime acceptance claim is supported.


## Frozen v4 source-ownership and evidence re-review

Reviewed `work/counter-integrity/direct-counters-v4`, including the actual
builder hook, graph override, span refiner, final mapping preservation assertion,
and pinned compiler lowering. Independently verified all input, generated-source,
compiler/library/binary hashes in its build metadata, plus both oracle metadata
sets. Frozen driver SHA-256:
`9ecc679c96cfb7f0a073bb7467877c4c9701ed8ca8abac2c6766c4ee439f4ae8`;
return marker hook:
`2d89a1a44ec1893e03519bc0486fae79dab1b9a7a6ba4c1524ac27b770f7225c`;
coverage entry:
`ee58bbd89c4a7ffa42cc5e1267bfc87fecb617a7958fbc784299678fa816db5c`;
span refiner:
`4bfe91e2b6e896d9cfa33562c0a527fa496510cf50997dbc41bb322e281b55df`;
wrapper binary:
`8d8b6bb725c987c917d884f6109c01b55e8e0ccd9e99ad3ffd59fc4d4d0e56ae`.

The raw builder remains authoritative for executable MIR. Its wrapper walks only
the owning HIR body, without entering nested closure/const/item bodies, and
recognizes lexical `return` from a HIR return expression. A marker is attached
only to the single non-cleanup Goto whose SourceInfo span exactly matches that
expression. The exact pinned `break_scope` lowering evaluates/assigns the return
RHS first, then the drop-tree entry link replaces its placeholder terminator with
Goto while retaining SourceInfo. Thus the marker measures reaching the return
transfer after RHS evaluation. It is not a promise that subsequent scope drops
or the caller complete normally. A real lexical return with missing/ambiguous
matching Goto aborts measurement. Synthetic/nonlexical spans retain stock behavior;
they are not silently converted to guessed return positions.

Both hooks compare non-coverage statements and terminators before and after.
Inspection agrees with those guards: only Coverage statements and mapping data
change. The original compiler pipeline, promoted fragments, source scopes,
eligibility and runtime crate dependencies remain owned by the compiler/runtime,
not by this diagnostic. No runtime implementation change is required by the
measurement repair.

The span refiner now preserves outer-span fragments rather than dropping the
whole parent when an inner span has another block owner. Every fragment retains
an actual contributing span's BCB. An endpoint sweep selects overlapping owners;
source-union preservation is asserted. Final code mappings are clipped to the
stock mapped byte union after genuine return markers were inserted, grouped by
expansion context, then exact union equality is asserted. This prevents v3-style
new enclosing-span coverage outside the canonical footprint while refusing v2's
loss of old source positions. It does not restore coverage with fabricated
counter values. Branch mappings are retained independently.

The actual four-case `footprint-comparison.json` has zero removed mapped byte
intervals and no missing/added functions. Line denominators equal stock:
reader 8,978, context 9,702, delayed EOF 8,972. Region counts increase through
subdivision. The added-source audit identifies additions as lexical return
keywords. Independently inspected `pool.rs:121` in actual v4 exports: the formerly
lost assignment now has region `[121,25,121,32,0,0,0,0]`. The previous demonstrated
v2 mapping-loss P2 is resolved for this candidate.

Also independently inspected actual reader regions: `io.rs:109`'s newly measured
return keyword is `[109,25,109,31,0,0,0,0]` in the panic case and has count 1 in
normal EOF. Both unchanged old side-effect oracles report PASS in
`oracle-v4-evidence`; all three assignment/return cases report PASS in
`assignment-v4-evidence`. Their metadata links the reviewed binary and unchanged
fixture/validator hashes. These include process exit across calls and RHS
interruption, so preserving the footprint is accompanied by behavioral evidence
about counter ownership rather than merely matching a denominator.

No new P1/P2 DDD/source-ownership finding is present. This remains a bounded
calibration: the actual exports contain no branch records, so matching branch
inventories does not dynamically verify branch instrumentation. Optimized/inline
fixtures, full feature/source inventory, fork/exit/kill profile persistence,
other platforms, integrated mechanical checks and scoped/full performance still
need their own evidence. Stock-unmeasured constructs stay explicit inventory gaps
under the no-exclusions target. Compiler-private dependencies must remain in
measurement tooling if this proposal is integrated. No builds, tests or production
edits were performed by this reviewer.


## Frozen V8 independent source-ownership re-review

Reviewed the separate `work/counter-integrity/builder-v8`,
`direct-counters-v8` and actual linked `backend-v8` source trees. The backend
metadata and wrapper link command identify V8; comparison against backend-v7
shows its only Rust-source difference is additional provenance logging. This
reviewer authored none of these implementations and ran no builds or tests.

Independently checked all 123 entries in `direct-counters-v8/freeze-audit.json`
against current file bytes. Reviewed driver SHA-256:
`aba80f5cd4d52d3829259671a5d6f52c04c10620d45434da9060915f3559a35c`;
coverage entry:
`ac176ba376f4d02fd2e81b2b857effba854305f193dbe5cd0411968474779919`;
new builder ownership module:
`3ead78cc2c45621875a8a8a6612e84d3bab5bbb9ebcbb92d0c999f24b121342d`;
wrapper binary:
`4aa77a37e099ed3a02ae595a420132340805fd13dc690f1ca2021a47f27fc2c0`;
builder dylib:
`9c3df9c4feef95de75110034557d3e98ccaeda6d02a5934aa8f202f2feb56769`;
backend dylib:
`f78173bb4ee3d2fb1c783cd4fa7e4a7f7cad02fd7f92cf9f4cb369cf9ba45e93`.

### Ownership challenged

Compared every copied MIR-builder Rust file against the installed pinned compiler
source, not merely the authored summary. Besides the exported hook and new
ownership/provenance modules, changes initialize/finalize a per-body source-point
collector, enter it at actual expression/statement/block lowering routes, and
allocate coverage marker IDs through the compiler's existing marker allocator.
No executable branch, local, statement or terminator was changed in that diff.
Only the builder hook is exported; original type checking and other providers
remain with the linked compiler.

The source point is now owned by an identified THIR node under the owning body.
Macro selection follows expansion identity and parent relationships, allowing a
single structural root or a contiguous sibling statement sequence. Multiple
unsupported roots abort instead of selecting a convenient observed block. Nested
bodies own their own executions. The selector does not use runtime counts,
source filenames or specific oracle labels to choose ownership. Exact HIR leaf
spans and callee spans remain separate: leaf evaluation can precede argument
interruption, while callee dispatch belongs to the actual call terminator after
argument evaluation.

Each selected lowering point inserts only SpanMarker and BlockMarker, checked by
reconstructing the prior block after removing those two statements. The shared
marker allocator prevents collisions with branch markers. A registry keyed by
LocalDefId transfers typed marker identities to mapping generation. Raw block
numbers are diagnostic evidence only; promotion resolves unique marker IDs and
checks both span equality and completeness. Missing, duplicate or unsupported
owners fail compilation. The registry is compiler-process state, not a runtime
service locator or application dependency.

Mapping repair changes ownership only inside proven invocation-name source
bytes; surrounding mapped fragments remain. A source-union assertion checks that
no existing context loses bytes. Distinct typed points are not deduplicated using
coordinates or observed counts. Backend physical-file aliases key on expansion
context and physical source file, retaining root file 0; supplementary empty or
cross-file-invalid regions are rejected. This supports physical source coverage,
not an invented LLVM expansion UI hierarchy. The reserved zero-valued physical
slots for optimized-away branch outcomes preserve branch identity rather than
manufacturing an increment.

The new compiler-private builder/backend dependencies remain confined to ignored
measurement tooling. Runtime domain/application/infrastructure ownership and
public APIs are unchanged. If integrated later, these dependencies and the
provenance files must stay outside runtime crates. The frozen copy is verified;
future rebuild/integration still needs reproducible source/dependency identity
checks rather than trusting that a same-named local dylib is the reviewed one.

### Evidence and limits

The actual four-case footprint audit has no missing functions or removed source
bytes; function/line denominators remain equal to stock. Region counts grow through
subdivision and supplementary source points. `cross-v8-evidence/validation.json`
and `lazy-v8-evidence/validation.json` both report PASS with zero failures,
including skipped, interrupted and deferred execution. The backend identity
addenda correct stale V7 references without erasing the original runner metadata.

The 48-record executable-MIR preservation summary is entirely true, with unchanged
fixture hashes; it compares complete built MIR modulo Coverage statements across
five fixtures. This is compile-time preservation evidence, not another execution
cohort. The provenance inventory contains 5,660 builder points and 5,660 promoted
points, with no unsupported status. Five compiler-generated leaves with no source
bytes are explicitly inventoried; the failed earlier empty-span compile remains
preserved and the backend rejection was not removed to conceal it.

No P1/P2 DDD or source-ownership defect was found in this frozen V8 scope. Actual4,
cross/lazy and provenance counts do not prove every macro form, optimization mode,
branch shape or platform. The parent's remaining unchanged calibration suite,
helper process/profile persistence, full source matrix, mechanical/performance
gates and the user's no-exclusions 100% target remain separate obligations.
Stock-unmeasured constructs are still inventory gaps, not permission to shrink
the denominator or claim readiness.


## V8 extended evidence re-review: acceptance reopened

Read `work/counter-integrity/extended-v8-evidence/metadata.json`, standalone
validation/export/stdout and Tokio compiler stderr. This is the same frozen V8
candidate, and the metadata records unchanged source inputs. Old five calibration
cases and inline cases at optimization levels 0 and 3 passing do not override
the failures below.

**P2: duplicate ownership doubles async literal coverage.** Standalone macro/async
fixture compiles and exits zero, but six literal checks fail: `async_enter` and
`async_after_await` expect 2 and report 4; `async_rhs_panic`,
`async_rhs_complete`, `async_macro_enter` and `async_macro_second` expect 1 and
report 2. The reviewer inspected the actual function export at `macro_async.rs:76`:
function `_RNCNvCs8xDSHWNZcQZ_11macro_async11async_value0B3_` contains the identical
code region `[76,13,76,26,2,0,0,0]` twice. Actual stdout contains exactly two
`OBS async_enter` records. Thus the doubled result is not just an absent mapping
or an unjustified validator expectation: two exported ownership contributions
represent the same two literal evaluations.

The source ownership contract must distinguish a genuinely separate typed
execution from a supplemental mapping of an already measured source evaluation.
V8's unconditional addition of typed leaf points alongside canonical mappings
has not established that distinction for the coroutine case. Fixes must use
source/typed ownership evidence, never count equality, fixture names, coordinate
blacklists or altered expected observations. Deduplicating genuinely separate
repeated macro executions would create a different undercount and is not an
acceptable blanket repair.

**P2 acceptance limitation: Tokio compilation aborts on unsupported macro
ownership.** `direct-macro-tokio-opt0/compile.stderr` records exit 101 while
compiling `tokio-macros`; the adapter asserts at
`builder-v8/source/builder/coverage_points.rs:316` for an unsupported structural
macro root in `entry::parse_knobs`, in quote source line 626. Failing closed is
preferable to a guessed owner, but still prevents measurement of a required
supported dependency path. No Tokio test/profile/export was produced by that
attempt. Repair the structural selector with evidenced ownership; do not omit
this dependency, downgrade the assertion to silent skipping, or claim the run
passed based on unrelated fixtures.

Both findings block broad measurement acceptance. V9 is a separate diagnosis
lane and has not been reviewed here. Preserve V8 failures, rerun unchanged
standalone and Tokio calibration after any fix, then obtain independent review
of code and evidence before full-source coverage. No builds/tests or production
edits were performed by this reviewer.


## Reuse audit of existing own-C evidence at f51e972

Read-only audit against current HEAD
`f51e97269960c484d15efc17d90ef2323a3ae663` confirms both
`work/coverage-closure/native-final-evidence` and
`native-scanner-complete-evidence` remain reusable **for their owned C bridge
component scope**. No native build or test was rerun. Prefer the later complete
scanner evidence as the canonical record; preserve both historical runs.

Compared each full saved source manifest with current file hashes. The later
scanner-complete manifest differs only at
`crates/application/src/projection/tests/admission_rollback.rs` and
`crates/infrastructure/src/process/io.rs`. The older final manifest additionally
differs at the scanner and two Rust test files. Neither has a changed native
bridge, header, native build/coverage driver, fault/allocator fixture or terminal
contract source. All current tracked `scripts/native/` and terminal contract Rust
paths are present in both manifests. The recorded coverage-driver hash matches
current `scripts/native/coverage.py`.

Inspected the actual LLVM export and report: owner.c, checkpoint.c, view.c and
verification.c total **356/356 regions, 23/23 functions, 249/249 lines and 198/198
branch sides**, zero uncovered in each category. Both outcomes files record all
nine executables exiting zero: seven real terminal contracts, allocator contract
and the explicitly synthetic boundary contract. Recorded platform is
macOS-26.6-arm64; Clang own-C instrumentation uses atomic profile updates. Identity
records pin Ghostty revision `82232ecde55405559dec29c5466cb9e39938cb41` and native
library SHA-256 `1679af9dcd92e55d154a1206c40d4bef7c0f148d222157e7a1a84d1bcec83ecd`.

This is existing component evidence whose measured inputs are unchanged, not a
claim the whole current revision was re-executed. The recorded denominator is
only the four owned bridge C files. Rust, guardian and pinned native engine were
uninstrumented; other platforms, experiment C programs, whole-system integration
and performance are not certified. Headers are unchanged but have no separate
line/function denominator. MC/DC has zero records and is unmeasured. Synthetic
fault-path coverage is not a real-engine fault guarantee. The Rust custom-counter
calibration failures do not invalidate this separate Clang component result,
and this component pass does not excuse Rust/native-engine/platform gaps under
the user's no-exclusions 100% target. No new evidence warrants rerunning the
already-passed own-C cohort solely for the unrelated Rust reader/test changes.


## Frozen V9 fixes: independent re-review

Reviewed actual V8-to-V9 source diffs and successful evidence. All 123 freeze
checks independently match current files. Only `builder/coverage_points.rs`
changes within the copied builder, SHA-256
`11a0ee8705723daa821061bbd7b690a8319e68b5b354e94bf0905b0e8fdf698c`.
Coverage entry SHA-256:
`88bacd823274407c49d2500f5ae11c4b25730c60a6c42ca38000529b5257397e`;
wrapper binary:
`336994bee84af4752ac276756551631a5ce1d90a8cb85a5a1bb230f912721776`;
builder library:
`b1e9582b31fa9281c84deb43fc08ebdeeef67fd29a3cfee54e128833d19e5226`.
The actual linked backend remains unchanged backend-v8, SHA-256
`f78173bb4ee3d2fb1c783cd4fa7e4a7f7cad02fd7f92cf9f4cb369cf9ba45e93`.

The coroutine fix replaces overlapping canonical ownership for all proven typed
source points in the body context, including leaf evaluation, rather than only
macro entry. It retains surrounding fragments and the source-union guard, then
adds each distinct typed point once. Selection uses typed source identity, not
observed counter values or failed fixture coordinates. This closes the observed
canonical-plus-supplemental duplicate without discarding distinct repeated
expansions. Independently inspected the actual standalone export at
`macro_async.rs:76`: it now has one `[76,13,76,26,2,0,0,0]` region, matching the two
observations. The six previously doubled literal checks all pass unchanged.

Structural root support now permits contiguous sibling statements followed by a
final expression under the same block parent. It explicitly requires that an
expression is last in both the selected sequence and the parent's child list;
this does not become a generic common-ancestor guess. Entry remains at the first
selected lowering node. The constructor bridge uses a THIR Scope's HirId to
recover its exact HIR Call when the inner expression is a lowered Adt; the lost
callee's expansion ancestry participates in ownership selection. It does not
invent a runtime call for a constructor, scan another body's source, or change
executable lowering. The retained scope is entered before constructor arguments.
Unsupported shapes still fail closed.

The unchanged extended suite now reports all four compile/test/validator phases
passing: standalone macro/async, Tokio macro/async, and inline optimization 0/3.
Cross-file and lazy-entry validations also pass. Current actual4 source-footprint
comparisons have no missing functions or removed source bytes. Compile-only MIR
preservation evidence reports all 48 general bodies and all 18 Tokio-specific
bodies equal modulo Coverage statements, with unchanged fixture hashes.
Provenance has 9,978 builder and 9,978 promoted points, including explicit
constructor bridges and no unsupported status. These are inspected records, not
new executions launched by this reviewer.

Both demonstrated V8 P2s are resolved for frozen V9's reviewed calibration. The
remaining parent calibration/function-entry/canonical matrix, final feature
matrix, helper profile persistence, cross-platform coverage and performance
remain separate requirements. No production code, independent fixture
expectation, build or test was changed/run by this reviewer. The no-exclusions
100% target is still open.

## Frozen V10 exact expression identity bridge: independent DDD review

**Scoped pass; no new P1/P2 ownership or dependency-direction finding.** Reviewed
`direct-counters-v10/frozen-candidate.json`, the actual expansion, AST-lowering,
and MIR-builder sources, and `final-collection` evidence. Independently checked
167 generated-source, dependency, build-script and frozen-candidate hashes: all
match. The two 44,009-entry outer input manifests are identical. This is a
read-only inspection of captured executions; this reviewer launched no compiler,
Cargo build, test, or calibration command.

The new registry owns observation records only. The resolver adapter delegates
every trait operation with its original arguments and return type; its sole
observation occurs after stock `visit_ast_fragment_with_placeholders`. Expression
roots use assigned AST NodeIds, while MacCall placeholders are resolved through
compiler LocalExpnIds, with cycle, missing-target and incompatible-fragment
checks. Distinct expansion identities sharing an AST root remain a vector. No
synthetic source identity or root-namespace override is introduced. Absent and
unsupported expression fragments are separately represented in provenance.

The AST lowerer diff is a wrapper around the unchanged original implementation:
it passes the original expression through, records its original NodeId and the
returned full HirId, then returns that same result. HirId includes its owner;
matching does not collapse owners to local integer IDs. The builder queries
those identities from actual THIR Scope nodes, including the previously
reviewed typed constructor-callee relation. Duplicate views of one anchor are
deduplicated; multiple anchors for an expansion fail closed. The existing
structural selection remains first, avoiding duplicate supplemental mappings.
Source containment filters ownership only after identity matching; it is not
used to guess the AST-to-HIR relationship. The change therefore puts each fact
at its authoritative compiler boundary instead of inferring it in the backend.

Global registry state is acceptable within this standalone wrapper's explicit
one-compiler-session contract: begin asserts no active session and resets all
maps, capture must finish before lowering/read access, and end clears the maps.
A Mutex protects access across compiler workers. Failed compilation terminating
the wrapper cannot leave state for another process. This is not evidence for an
embedded multi-session compiler or recovery after a caught compiler panic.
Incremental reuse would invalidate the capture contract, and the parsed compiler
configuration rejects it before providers run. Preserved direct and response
file probes both exit 2 with no cache or output artifact.

The ten recorded commands all exit zero with incremental disabled. Inspected
identity-oracle output reports no failures; the pinned syn compile passes with
unchanged inputs. Executable MIR comparisons cover 48 general, 18 Tokio and
seven identity bodies and report no differences after removing Coverage
statements. These results close the failed statement-only initializer attempt
for the tested corpus. The registry's inventory alone does not prove complete
coverage of every possible macro fragment or desugaring; unselected/cfg-removed
fragments and bodies outside the measured cohort still require the independent
coverage inventory and canonical matrix. Performance, other platforms, complete
100% coverage, and separate quality/adversarial acceptance remain outside this
scoped source-ownership judgment. No production source or oracle expectation
was changed by this reviewer.

## Frozen V11 ownership and preservation re-review

**Scoped pass for the current fixes; no new P1/P2 DDD/source-ownership finding.**
Independently checked all 178 entries of `direct-counters-v11/frozen-candidate.json`;
all match. Manifest SHA-256:
`a027a27c97a358d40e40284209e0574df088791161a2a4d344903941bc6c7c08`.
Reviewed actual V10-to-V11 diffs in expansion lineage/resolver, AST expression
and format lowering, builder source points, raw-point capture, promoted coverage
integration, return-marker hook and driver. No build, test or calibration was
launched, and no production source or oracle expectation was edited.

The terminal ledger closes V10's missing-root accounting gap for captured bang
expression roots. It starts from captured original AST identities, resolves to
owner-bearing HIR identities or documented typed projections, classifies using
the compiler's coverage eligibility, and reconciles selection, emission and
mapping receipts. Unknown eligible roots, missing emissions and missing runtime
mappings fail the session. Coverage-disabled, metadata-only, attribute metadata,
and stock coverage exclusions are explicit separate statuses. Non-expression
fragments retain a separate inventory and existing structural path; this is not
a claim that expression receipts alone prove all fragment coverage. The resolver
still returns the original result and observes eager invocations only after
successful resolution. Eager metadata receipts follow recorded parents and typed
attribute consumption rather than borrowing a nearby runtime source span.

The AST namespace boundary is now recorded from the stock resolver's next NodeId
without allocating a new ID. Captured roots and format projection anchors must
fall below that boundary. Format flattening and literal removal retain the
stock transformation and argument order, adding observations at the exact typed
removal sites. Erased entry projects to the next retained argument, through
further erased projections if necessary, or to the generated format constructor
after arguments. The projection relation fails on cycles or missing anchors.
The actual HIR parent determines body ownership, replacing the older source-span
containment test for exact identities. No observed runtime count determines
selection or exclusion.

The final forwarding repair supersedes an enclosing structural selection with
the exact shared original expression entry. The preserved futures-channel
records show parent expansion 65 and child 66 sharing NodeId 7508 and the full
HIR identity; the failing selection split bcb0/bcb26, while the repaired mapping
places both at bcb26. Runtime identities, emitted markers and mapping receipts
remain distinct. Source exposure is shared only for a recorded single forwarding
placeholder with identical physical bytes and the expected child hygiene parent.
The mapper asserts equal runtime owners before exposing those bytes once. This
is a typed alias rule, not coordinate-only deduplication of repeated invocations.

Unreachable zero mappings are owned by the graph's non-executing sink, with
assertions that it receives no increment or branch counter and remains CovTerm
Zero. Promoted operational reachability ignores only imaginary false edges;
raw proof additionally applies the pinned compiler's inhabitedness predicate to
impossible call returns while retaining cleanup edges. Raw marker identity,
span, original block, statement position and cardinality must all reconcile
before a subsequently erased marker may use that proof. No source bytes are
removed merely because a marker has disappeared. The strict paired stock
comparison reports zero missing raw function identities and zero removed
function/file/region-kind intervals in all four actual regression cases. Their
function and line denominators match stock; supplemental region counts increase.

Inspected final evidence: control-flow validation passes 244 source checks;
formatting passes 555 source checks and 12 terminal checks, including erased
literals around panic and argument evaluation order. Cross-file, lazy-entry and
identity reruns pass. The 48/18/7 MIR comparisons report no executable changes;
the additional Tokio comparison reports 2,512 common bodies, no missing/new
bodies and no differences. The latter is comparison against the preserved
original V11 failure, not an independently rebuilt stock compiler cohort.
Pinned syn and Tokio compilation receipts report unchanged inputs. V10's older
44,009-input claim is not attributed to these V11 executions.

Remaining limits: captured terminal receipts and these bounded runtime cases do
not establish the complete canonical no-exclusions matrix, cross-platform
coverage, performance acceptance, or the final quality gate. Control-flow
fixture output does not by itself prove a marker was present in every cleanup
or coroutine-drop block. Global proof/lineage storage continues to assume the
standalone one-session wrapper; embedded compiler reuse and recovery from a
caught compiler panic are unverified. Incremental reuse remains explicitly
unsupported. Separate quality/adversarial decisions remain required.

## Frozen V12 single-expression statement root re-review

**Scoped pass; no new P1/P2 source-ownership finding.** All 287 entries of the
frozen candidate independently match; manifest SHA-256 is
`c9514ff3678bd7d17e6456e9a56d15ba272a62cad79ca391587d7c1ff9e7b16d`.
Reviewed only the actual V11-to-V12 ownership repair and supporting evidence.
No build, test, prototype, production change or oracle edit was performed.

Expansion now recognizes a statement fragment containing exactly one Expr or
Semi as that existing expression root. It records the distinction in
`root_origin`. This is the correct owner for this fact: the expansion adapter
can inspect the typed original fragment before lowering. No ID is synthesized,
no statement list is collapsed heuristically, and a semicolon adds no separate
entry evaluation. Other statement shapes retain their previous classification.
The original root validation, placeholder-chain resolution, owner-bearing HIR
relation, exact THIR entry and terminal accounting remain unchanged. Compared
all source files in the lowering and builder components: no source changes.
The coverage integration adds only a diagnostic receipt before the existing
missing-parent failure; both missing-parent and equal-owner assertions remain.

Preserved repair evidence shows parent expansion 88 and child 234 resolving to
original NodeId 12625 and the same full HirId with local ID 2303 in `run`.
Compiler body-parent lookup places that expression in
`release_load_support::phase::run::{closure#0}`. Both select Expr(e2474), retain
separate markers 225/226 and terminal mapping receipts, and map to bcb612 before
the child shares the parent's lexical source exposure. This fixes the absent
parent rather than weakening the alias contract or borrowing a nearby block.
The terminal summary reports 261 runtime roots out of 295 captured fragments,
zero unsupported roots, with the remaining 34 separately inventoried fragments.

The exact failed release_load compile/link receipt passes; its 631-entry input
manifests match before/after. The 53 MIR bodies emitted before V11's failure are
preserved modulo Coverage statements. The 166 additional completed bodies have
no old counterpart and are not claimed as comparative preservation evidence.
The unchanged focused control-flow and formatting validator results both pass.
This repair does not expand V11's bounded source-preservation evidence into a
completed full canonical coverage result. The running canonical matrix,
independent quality/adversarial review and previous platform/performance limits
remain separate acceptance requirements.
