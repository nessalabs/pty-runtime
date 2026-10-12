# Independent quality review of the direct-counter diagnostic

Latest verdict (V10 canonical matrix): **general acceptance remains blocked by
the P2 Tokio source-point ownership compile failure**. The footprint-validation
P2 is now closed for the four paired actual cases after a strict same-source,
same-configuration comparison. Bounded footprint preservation and 73 unchanged
MIR bodies do not override the canonical compile failure. Chronological findings
retain all prior failures and scoped results.

## Reviewed scope and design

The reviewed ignored prototype is
`work/counter-integrity/direct-counters/driver.rs`, SHA-256
`e7f3facd0c2984926357b9e887cdbd513948277af3b064516e89063f750dfc0d`,
and its builder is `build.py`, SHA-256
`4ad0c6b9c3b4cdace1b1dbc1584c279c63cc9e34e0b224834372139114fc9b03`.
This review read the exact Rust 1.98.1 query provider, upstream Rust issue
164153, prototype source, build metadata, actual-test artifacts and independent
oracle source/validator/exports. The reviewer ran no builds, runtime tests or
benchmarks and made no production changes.

The adapter has a narrow responsibility: override `coverage_ids_info`, retaining
stock coverage eligibility and source mappings. It copies the stock final-MIR
scan and exclusion of inlined/inlined-parent scopes. It assigns physical
counters to the intersection of mapped and surviving BCBs, assigns Zero to
mapped blocks removed by optimization, and emits no derived expressions. The
compiler owns mappings; this adapter owns counter allocation. The isolated
compiler adapter is an appropriate boundary, with no runtime dependency or
unnecessary framework. Builder identity checks and coverage calculation remain
separate concerns. Mandatory atomic increments address concurrent counter
writes independently of the invalid flow equations.

## P2: compiler identity mismatch — fixed and re-reviewed

The initial implementation validated the selected external `rustc -Vv` while
executing the wrapper's linked `rustc_driver`. A rebuild against another
compatible private API could have passed the external check while executing a
different compiler.

The revision adds an in-process linked-driver version assertion. The builder
requires full commit `48a229ceaefd4985c50990b14116b6d856af0985`, fixes the actual
compiler/sysroot/library path, records source/builder/compiler/library/binary
hashes, and compares linked-driver `-Vv` output byte-for-byte with the selected
compiler. The selected-compiler full-commit check remains. Re-review closes this
finding for the current macOS diagnostic scope. The wrong-toolchain rejection
is retained in `mismatched-toolchain.log`. These checks are not a declaration of
support for other compiler versions or platforms.

## Actual unchanged-test evidence independently checked

The six stock/direct selected executable hashes match `actual-metadata.json`.
Each of eight per-case directories contains one raw profile and a log with one
passing test. All recorded build/run/merge commands exited zero. The independent
review compared full function names, filenames, region coordinates, kinds and
indices while excluding only count values; stock/direct inventories are equal:

| Case | Functions | Regions | Counter result |
| --- | ---: | ---: | --- |
| Reader observer panic | 1,427 | 15,462 | False EOF return at io.rs:109 changes 1→0; its enclosing guard stays 0; saturated siblings become 0 |
| Normal reader completion | 1,427 | 15,462 | Actual EOF return remains 1 |
| Context collaborator panic | 1,133 | 16,541 | Saturated context.rs:178–179 become 0 |
| Delayed accepting EOF | 1,435 | 15,521 | Actual release/continue path at io.rs:114–116 remains 1; EOF return remains 1 |

These paired exports retain all mapped regions and functions in the checked
fixtures. They are not the full feature matrix or proof that every stock BCB
mapping measures partial execution correctly.

## P2: unexecuted continuation still reported executed — open

Evidence: `work/counter-integrity/oracle-evidence/direct/process_exit/` and
`work/counter-integrity/oracles/process_exit.rs`.

The independent fixture prints `exit_main_enter`, `exit_interrupted_enter`, then
`exit_condition_enter` before `process::exit(23)`. Main's next call,
`observe("never_main_after_exit")` at line 30, never executes. Nevertheless the
direct-counter export explicitly maps both its call token (columns 5–12) and
literal (columns 13–36) with count 1. This is not a validator gap fallback:
the full function region records themselves contain these positive counts.
`validation.json` correctly fails both positions, expected 0 / actual 1.

The normal/panic independent oracle passes with the adapter, and both stock
oracles fail as retained negative controls. The remaining direct process-exit
failure demonstrates that assigning physical counters to existing coverage
blocks does not alone establish source-region execution after an interrupted
call. Partial execution within the stock mapping must be addressed and the
unchanged oracle must pass before acceptance. Do not hide this by editing the
fixture, filtering continuation regions, clamping counts, or claiming that the
absence of saturation validates the report.

## Remaining work

Preserve this failed calibration and investigate the mapping/counter placement
boundary before proposing a revised measurement implementation. A revised tool
requires independent re-review and the same before/after calibration. Explicit
inline, optimized-away and branch-mapping fixtures remain necessary, as do
guardian fork/exit/kill persistence checks, fresh full-matrix artifact provenance
and the ordinary mechanical/performance gates. The present result does not
qualify whole-project coverage, integration readiness or a 100% claim.

## V2 re-review: interrupted-call repair passes, mapping scope regresses

The separately retained V2 prototype is
`work/counter-integrity/direct-counters-v2/`. Its driver SHA-256 is
`3a296a79c27ee6ef7a0a501ebf9965fe40a6ac30d7a754dc689541545e25e47c`,
builder `3fbd184f312477ab550b9f74fb0b9a980f50c106a4fdac3f9aeb396d7fbade63`,
and binary `d4272f107e1ab949812311001b1a4ab4ff625f70b6bb77e32e1ea6ab7f6f8939`.
Independent rehashing matches every identity in its build manifest. V1 and its
failed calibration remain preserved; V2 has not been accepted.

V2 wraps the original `mir_promoted` query after stock instrumentation. It only
re-instruments bodies that already have stock coverage information, removes
only old VirtualCounter statements, retains original executable statements and
terminators, and checks the original function source hash. Nine compiler-source
files are hash-pinned before generation; the graph patch restricts chaining to
Goto/FalseEdge/FalseUnwind so interruptible Call/Drop/Assert/InlineAsm normal
continuations get separate coverage blocks. The direct physical-counter query
and compiler identity safeguards remain. This is a coherent placement fix at
the compiler boundary; it does not insert fake runtime branches.

Both unchanged independent oracles now pass in `oracle-v2-evidence`: the
normal/panic case retains expected positive and zero counts, and the process
exit case correctly reports the unexecuted main continuation as zero. This
closes the V1 interrupted-call finding for the demonstrated oracle. Four actual
runtime cases also passed. These successes do not close the new scope defect.

### New P2: re-extraction removes previously mapped source — open

V2's `footprint-comparison.json` records loss of source interval unions by
function, file and region kind, with no added footprint groups:

| Case | Groups losing source intervals | Stock lines | V2 lines | Stock regions | V2 regions |
| --- | ---: | ---: | ---: | ---: | ---: |
| Reader panic | 741 | 8,978 | 8,516 | 12,952 | 14,373 |
| Reader normal | 741 | 8,978 | 8,516 | 12,952 | 14,373 |
| Context panic | 662 | 9,702 | 9,277 | 15,077 | 16,928 |
| Delayed EOF | 742 | 8,972 | 8,516 | 12,957 | 14,370 |

Function inventories are unchanged, but increased region counts do not prove
preserved source scope. Independent source inspection confirms lost intervals
include executable syntax: `scheduling/pool.rs:121` loses columns 25–32
(` = true`), and line 124 loses assignment syntax. Entire previously mapped
source lines also disappear, including field initializers. This cannot be
accepted merely as removal of whitespace or braces.

The author's investigation identifies the stock span-overlap filter: its
`pending.retain` drops outer spans when overlapping child spans belong to a
different BCB. V2's finer graph exposes that behavior broadly. A repair must
retain every required source obligation and assign it to the correct physical
execution point; it cannot restore an outer region's falsely positive count
over an unexecuted child continuation. The existing two oracles and all original
failed evidence must remain unchanged. Review remains blocked pending that
scope-preserving repair, footprint evidence and independent re-review.

## V4 independent re-review and finding closure

V3 restored source unions but was not accepted: restored outer spans could
introduce misleading punctuation ownership, and the independent assignment/
return oracle rejected missing explicit-return keyword mappings. Its original
failures remain under `assignment-v3-evidence`. V4 is separately retained under
`work/counter-integrity/direct-counters-v4`; earlier versions are unchanged.

| V4 input | SHA-256 |
| --- | --- |
| Driver | `9ecc679c96cfb7f0a073bb7467877c4c9701ed8ca8abac2c6766c4ee439f4ae8` |
| Builder | `64eedb90599dbdcf1ecbe99907c9bc227f5a371cf39323491a7a5997176c7a41` |
| Coverage entry | `ee58bbd89c4a7ffa42cc5e1267bfc87fecb617a7958fbc784299678fa816db5c` |
| Span refinement | `4bfe91e2b6e896d9cfa33562c0a527fa496510cf50997dbc41bb322e281b55df` |
| Return markers | `2d89a1a44ec1893e03519bc0486fae79dab1b9a7a6ba4c1524ac27b770f7225c` |
| Executed wrapper | `8d8b6bb725c987c917d884f6109c01b55e8e0ccd9e99ad3ffd59fc4d4d0e56ae` |

The reviewer independently rehashed every build-manifest input, linked compiler
library, wrapper binary and generated source file. Oracle metadata binds both
oracle suites to this wrapper hash. The four original oracle inputs and three
assignment-oracle inputs have identical hashes to the earlier recorded runs and
still match the current files; no assertions or expected counts were weakened.

### Implementation and responsibility boundaries

The graph-boundary fix remains. Span refinement uses an ordered sweep with
source-span ownership, retains enclosing fragments after nested spans, and
asserts fragment bounds and input/output source-union equality. The mapping
entry then intersects these mappings with the canonical stock source union,
grouped by expansion context, and asserts that no canonical mapped bytes were
lost. This retains stock obligations without introducing arbitrary outer
punctuation as new measured obligations.

Explicit `return` keywords are added through a separate provenance adapter at
the original MIR-build hook. It visits eligible function HIR without borrowing
closure/const-block returns, verifies the actual source keyword, and requires
exactly one non-cleanup Goto with the matching return-expression source span.
The marker occurs after the RHS has assigned the return place. An unsupported
or ambiguous matching return fails compilation. This supplies a real execution
anchor instead of inferring successful return from an enclosing source range.

Both return-marker insertion and re-instrumentation compare executable MIR
before and after, excluding only coverage statements. Existing executable
statements and terminators remain unchanged. Stock eligibility, version/source
identity checks, mapped-but-optimized-away Zero terms, inline-scope filtering
and atomic direct increments remain. The responsibilities are separated into
compiler binding, graph/mapping repair, explicit-return provenance and counter
allocation; no runtime dependency or unrelated abstraction was introduced.

### Independent evidence checks

Both original normal/panic and process-exit oracles pass in
`oracle-v4-evidence`. All three unchanged assignment/return cases pass in
`assignment-v4-evidence`: normal execution with caught RHS panics, exit during
assignment RHS, and exit during return RHS. The latter validator checks every
character of LHS, RHS, constructor, return keyword and closing-boundary tokens
against independent state/side-effect observations. Uncompleted assignments,
outer constructors, returns and function boundaries remain zero.

The reviewer independently rehashed the actual-test executables, confirmed each
of four case logs has one passing test and one raw profile, and recomputed full
coverage of every old source interval by V4 intervals. None is lost. No function
or source file is removed; additions contain only explicit `return` keywords.

| Case | Stock/V4 functions | Stock/V4 lines | Stock regions | V4 regions |
| --- | ---: | ---: | ---: | ---: |
| Reader panic and normal | 1,125 | 8,978 | 12,952 | 20,151 |
| Context panic | 996 | 9,702 | 15,077 | 23,265 |
| Delayed EOF | 1,129 | 8,972 | 12,957 | 20,127 |

V4's reader-panic guard call, explicit return keyword, EOF value and retained
release path are zero. Normal completion reports the return keyword and value
once; delayed accepting EOF reports the release path once and subsequent EOF
return once. The stock-unmapped `if` keyword remains unmapped rather than being
invented as a new positive obligation. The V1 incomplete-invocation and V2
source-loss findings are closed for this reviewed implementation and evidence.

No remaining P1/P2 was found in this scoped review. Whole-matrix coverage,
guardian persistence, explicit optimized/inlined/branch configurations and
tracked integration still require their own validation. The actual fixtures
have zero branch-mapping functions, so their equal branch inventories are not
evidence that branch-coverage mode was tested. Compiler/version/platform scope
remains pinned to this diagnostic build. No whole-project 100% claim follows
from these calibrations, and no new builds/tests/benchmarks were run by this
reviewer during re-review.

## V5–V7 review: compiler bridge verified, cross-file obligations remain open

V4's later matrix compilation failed closed on repeated macro returns sharing
an identical expanded Span. V5 groups exact HIR spans and requires matching raw
completion cardinality, retaining the group identity rather than inventing an
individual HIR-to-MIR pairing. Its pinned builder-source evidence and failure
remain recorded. V6 added supplementary macro-return mappings and reserved
unincremented physical slots for optimized-away branch outcomes so LLVM would
not silently omit their source branches. Its same-file supplementary mappings
then exposed another issue: sorting a macro definition before the owning
function changed the exported function entry count. V6's token-level success
was insufficient, and that failed evidence remains preserved.

### V7 backend and provenance review

V7 selects the copied Rust LLVM backend through the public compiler
configuration constructor. It uses the pinned Rust 1.98.1 compiler ABI and exact
matching native LLVM library. Independent comparison of baseline and V7 source
trees finds only coverage `mapgen/covfun.rs`, the diagnostic provenance module,
and crate-link glue changed. Every generated-source, native-object, driver and
backend binary hash was independently verified against its manifest. Baseline
and V7 use identical original compiler-source hashes, native shim object/archive
hashes and LLVM library hash.

- Baseline backend: `e70e23a365b6c95e5073fe73e330a7d04eb1bdc8d201dfd9398c1d91b93a7687`.
- V7 backend: `d2d0b93489060c805dcdb63e965da77810e14a0c0d4f8957c9f08724275034db`.
- V7 driver: `4e1ecd30a0f0336da71d238bdc148a2064d8669258b78d607971a5f8f64d1975`.

The reviewer independently compared all three retained unmodified-backend proof
cases: stock/custom binaries and full export JSON are byte-identical; LLVM IR
matches except the ModuleID comment. Separately, V6/V7 `normal_and_panic` and
`macro_async` IR matches after removing only ModuleID and serialized coverage
mapping globals. Physical counters, increment instructions, executable IR and
other metadata are retained in that comparison. These bounded proofs establish
the compiler bridge and the scope of the V7 backend change, not all compiler
configurations.

V7 keeps ordinary function mappings in virtual file 0 and assigns supplementary
definition mappings later IDs keyed by actual physical file and expansion
context. Coordinates are calculated against each actual source file. It adds
no fake expansion entry regions: return completion cannot stand in for entry.
Missing graph ownership, mixed ordinary contexts, cross-file spans and invalid
supplementary coordinates fail closed. Structured provenance retains exact raw
HIR groups, completion blocks/scopes/predecessors, promoted marker ownership,
final physical IDs and codegen virtual-file/coordinate/counter associations.
This separation of compiler ownership evidence from exported counts is sound;
the sidecar does not repair or rewrite reported values.

The five original modes, four extended configurations (including inlining and
optimization), and two new macro function-entry calibrations all pass in the
retained V7 evidence. In particular macro/root entries now report 3 and 3,
closing the bounded V6 entry-count defect. The reviewer read the validation
records and compiler-source/IR evidence without performing new builds or runs.

### New P2: cross-file definition bodies and invocation entry — open

`cross-v7-evidence/validation.json` records 25 failures. The independently
specified two-file fixture prints exact observable call sequences, checks
returned values, catches the intended RHS panic, and declares positive and zero
counts before collection. Its source and validator independently locate tokens
and function signatures; functions are not selected using the expected count.

Macro-definition observation calls in `cross_file_macros.rs:6` and `:15` execute
2 and 3 times respectively, but have no source coverage mapping. At the root
invocation name, `cross_file.rs:15` reports 1 while the invocation is entered 3
times, established by the exact logged sequence. Corresponding invocation
failures account for the remaining mismatches. Definition return keywords and
function entries pass, but those are separate completion and entry obligations
and cannot substitute for the missing/miscounted source tokens.

The oracle's requirements reflect the fixture's public effects; no assertions
need weakening. Preserving existing stock bytes alone cannot establish complete
coverage when stock macro span normalization already omits definition-body
obligations or associates an invocation with normal completion. V7 must not
support a whole-project 100% claim. A typed expression-entry provenance repair
requires separate review, unchanged old and new calibrations, complete source
scope/virtual-file checks and executable-MIR/IR preservation evidence. No V8
implementation is approved by this V7 review.

## V8 independent review: typed source points and broader calibration failures

V8 adds a pinned copy of the MIR builder, exporting only its original build hook
and typed source-point metadata. Its lowering changes insert Coverage statements
at selected THIR expression/statement/block boundaries; they do not add runtime
branches. Macro entry is selected from a structural root or a contiguous sibling
statement sequence, not from a dominance guess or observed count. Literal/path
evaluation points and callee dispatch have separate ownership. Construction of
a closure/future is consequently distinct from executing its body, and a call's
dispatch remains after argument evaluation.

Each source point allocates a BlockMarkerId in the existing shared coverage
marker namespace. Promotion requires a unique marker with unchanged source
span, a valid BCB owner and complete marker cardinality. The registry retains
raw block/statement indexes only as evidence; they do not identify the promoted
owner. Per-insertion reconstruction asserts that the original block remains
identical after removing the two new Coverage statements. The existing return
provenance, full source-union assertions, physical counter allocation and backend
file ownership remain in place. The V8 backend differs from V7 only by logging
invocation-name mapping provenance.

This is an appropriate separation of structural source ownership, identity
transport, counter allocation and codegen. The broad source selection policy
still needs calibration: a valid marker does not alone prove that adding its
mapping preserves unique ownership of source already covered elsewhere.

### Independently checked evidence

- Driver: `4aa77a37e099ed3a02ae595a420132340805fd13dc690f1ca2021a47f27fc2c0`.
- MIR builder: `9c3df9c4feef95de75110034557d3e98ccaeda6d02a5934aa8f202f2feb56769`.
- LLVM backend: `f78173bb4ee3d2fb1c783cd4fa7e4a7f7cad02fd7f92cf9f4cb369cf9ba45e93`.
- Source-point implementation: `3ead78cc2c45621875a8a8a6612e84d3bab5bbb9ebcbb92d0c999f24b121342d`.

The reviewer independently rehashed all 123 frozen inputs and the builder's
private dependencies. A fresh read-only comparison of all 48 retained MIR bodies
found V7/V8 identical after removing only Coverage statements. All four actual
runtime logs contain one passing test and one raw profile; an independent
interval comparison found no lost original source region in any case. Original
function/line denominators remain unchanged, while region counts increase.
The provenance inventory contains 5,660 builder points and 5,660 promoted points
with no unsupported status in that bounded actual-test collection. Five
synthetic dummy/empty source leaves have no source bytes and are explicitly
identified; the initial failed attempt is preserved separately.

The unchanged cross-file oracle now passes, closing V7's demonstrated missing
definition calls and wrong invocation-entry counts. Its original four source,
expectation and validator hashes still match V7. The independent lazy-entry
oracle also passes. It checks public atomic state, returned values and exact
stdout for an uncalled closure, two constructed futures with one poll, a zero-
iteration loop, an unexecuted short-circuit RHS, an argument panic before callee
dispatch, and three repeated invocations. These tests challenge distinct
execution points rather than mirroring the marker algorithm.

### P2: async source aliases double literal counts — open

The broader unchanged standalone macro/async oracle fails six literal-count
checks in `extended-v8-evidence/direct-macro-standalone-opt0/validation.json`.
For example `async_enter` is observed twice, but the literal reports four. The
reviewer inspected the full function export: the `async_value` coroutine contains
two identical regions `[76,13,76,26,2,0,0,0]`, both in virtual file 0. The
aggregate therefore counts one execution twice. Five other async literal checks
show the same factor-of-two error.

Source context differing from the HIR body context can arise from coroutine
desugaring as well as previously unmapped macro-definition source. Appending a
typed leaf point without resolving its overlap with the canonical mapping adds
duplicate ownership. A repair must establish one correct owner for the same
execution while preserving genuinely distinct repeated expansion points; generic
span/count deduplication is not justified. The unchanged oracle must pass after
the repair. A full source union does not prove correct counts.

### P2: valid Tokio dependency fails structural-root support — open

The unchanged Tokio configuration fails compilation with exit 101 in
`extended-v8-evidence/direct-macro-tokio-opt0/compile.stderr`. V8 rejects a macro
root in `tokio-macros`' `entry::parse_knobs`, originating at `quote`'s
`lib.rs:626`, expansion context #132. Failing closed is appropriate, but the
requested matrix cannot complete with this implementation. The author's
subsequent inspection identifies a contiguous sibling statement sequence plus
its block's final tail expression, a structure V8 does not accept. Supporting
that structure requires a bounded structural rule and unchanged calibration;
excluding the dependency or inventing an ownership point would not close this
finding.

The five original modes and inlining configurations at optimization levels 0
and 3 pass, but they do not waive either broader failure. No general measurement
acceptance or 100% claim is supported by V8.

### Evidence bookkeeping limitation

The original V8 cross-file/lazy per-run `input_hashes` mistakenly list the V7
backend library instead of V8's linked backend. The frozen V8 driver/build
manifests do bind the actual V8 backend, and all current hashes match; this is
not evidence that the wrong backend executed. It does mean those per-run
inventories alone do not prove the actual backend stayed unchanged across the
run. Preserve them, add an explicit correction, and derive subsequent run
inventories from the wrapper's linked dependency manifest. V9 remains a new
candidate requiring separate review and unchanged calibration.

## V9 independent re-review and scoped closure

Frozen candidate: `work/counter-integrity/direct-counters-v9`, with builder
`builder-v9` and the unchanged linked `backend-v8`:

| Artifact | SHA-256 |
| --- | --- |
| Driver | `336994bee84af4752ac276756551631a5ce1d90a8cb85a5a1bb230f912721776` |
| MIR builder | `b1e9582b31fa9281c84deb43fc08ebdeeef67fd29a3cfee54e128833d19e5226` |
| LLVM backend | `f78173bb4ee3d2fb1c783cd4fa7e4a7f7cad02fd7f92cf9f4cb369cf9ba45e93` |
| Coverage entry implementation | `88bacd823274407c49d2500f5ae11c4b25730c60a6c42ca38000529b5257397e` |

### Source-change assessment

V9 replaces canonical ownership within every proven root-context source point,
including literal/path evaluation points, instead of only macro invocation
points. It preserves surrounding fragments, retains each distinct typed marker,
and still asserts that the source union is preserved. It does not pool observed
counts or deduplicate distinct runtime points. This resolves the demonstrated
coroutine alias by removing the competing canonical mapping for the same source
obligation before appending its proven owner.

The builder accepts a contiguous sibling statement sequence followed by an
expression only when that expression is the parent block's actual final child.
It still rejects separated roots or arbitrary branch-dependent collections.
An additional V9 compile attempt found a macro callee removed by THIR's enum
constructor lowering. The final repair uses the retained Scope's exact HirId:
only a HIR Call whose callee is a Path and whose scoped THIR value is an Adt
qualifies. The Scope is the real entry before constructor arguments. No span
containment, observed-count ordering or dominance guess supplies this identity.
The failed intermediate attempt remains preserved.

Only `builder/coverage_points.rs` differs between the copied V8 and V9 builder
source trees. Marker identity transport, callback boundaries, physical counter
semantics and the existing executable-MIR/source-union assertions remain. These
changes preserve the ownership boundaries established in the previous review;
no new P1/P2 was found in this bounded source review.

### Evidence independently checked

The reviewer rehashed driver/build inputs, generated compiler sources, the
builder library and its private dependencies. Corrected cross-file and lazy
run inventories each bind 21 inputs; the extended inventory binds 51. All match
current hashes, and the inventories now include the actual linked `backend-v8`
and `builder-v9` identities from wrapper build metadata. Original erroneous V8
inventories and their limitations remain preserved.

A fresh read-only comparison of the retained dumps confirmed all 48 ordinary
and 18 Tokio-specific V7/V9 built-MIR bodies equal after removing only Coverage
statements. All four actual-test executable hashes match metadata, every case
has one passing test and one raw profile, and the full footprint comparison
retains every original mapped byte and function.

All four unchanged extended configurations pass: standalone macro/async, Tokio
macro/async, and inlining at optimization levels 0 and 3. Both macro function-
entry checks pass. The reviewer inspected the formerly doubled async literal:
V9 has one `[76,13,76,26,2,0,0,0]` region, giving the required count 2. All six
former literal mismatches are absent. Tokio compilation and validation complete
successfully under the repaired structural rules. These results close the two
demonstrated V8 P2 findings.

The unchanged cross-file and lazy-entry validators also pass in the author-run
V9 evidence, retaining their public-effect/state assertions. These scoped passes
permit the coordinating task to continue broader validation. They do not replace
the remaining original-suite confirmation, canonical full feature matrix,
continuous-profile persistence calibration, mechanical/performance gate or
tracked integration review. No build, binary execution or timing experiment
was run by this reviewer; the re-review used source and retained evidence only.

### Subsequent canonical V9 matrix: new P2, unsupported dependency macro

The coordinating task's full run is preserved in `work/counter-integrity/full-v9`.
It completed in 37.579391956 seconds with `passed: false` and unchanged source
inputs. The reviewer read all three workspace logs: all-features, raw and
event-stream compilation fail closed in `syn 3.0.5` at
`gen/visit_mut.rs:1290` and `:3549`, rejecting structural macro roots in
`visit_expr_mut` and `visit_type_param_bound_mut`. This prevents the required
canonical matrix from completing. The earlier bounded V9 passes remain valid
within their scope and do not waive this new P2.

A further structural repair requires an exact ownership argument, unchanged
calibrations, complete matrix execution and independent re-review. Excluding
the dependency or silently dropping an unsupported source point would not close
the finding. The separately checked surviving-parent continuous-profile cases
pass within their persistence scope, as recorded in
`continuous-profile-oracle-quality.md`; they do not establish whole-project
coverage or remove the compile blocker.

### Additive identity-macro oracle: independent pre-execution review

The V10 investigation is challenged by a new independent fixture whose macro
expands only its argument. It requires source invocation entries of 1 for a
direct expression, 3 for a repeated statement, 1 before an argument panic, and
0 for a skipped statement. The returned value 15, atomic total 11 (5 + 3×2),
exact observation sequence and caught panic establish these expectations without
consulting compiler markers or coverage output. The validator checks every
`identity!` byte, positive/zero observation calls and literals, and function
identity by symbol plus signature location before comparing counts.

No P1/P2 was found in the pre-execution fixture/specification/validator review.
This is not an execution or V10 acceptance result; existing fixtures and typed
lowering/source-footprint preservation remain required.

| Input in `work/counter-integrity/oracles` | SHA-256 |
| --- | --- |
| `identity_macro.rs` | `60c4648c6d45b07d2c08540143279019830786a19e4c26105aa22cab34b7d040` |
| `identity_macro_expected.json` | `82dd655d06580e93c1668601de6541149739dbf64e59847327ee746302233d05` |
| `validate_identity_macro.py` | `eed10fc30b779e7161caf9e40b199d061b8f30dc9d898b2f9b2c147afcd5e0be` |

## Guardian V9 diagnostic: compiled source ownership audit

The original guardian coverage driver exited 1 at its source-denominator
assertion. This failure remains a failure; no report exclusion or threshold
change was applied. An independent read-only audit of
`root-guardian-v9-evidence` and `root-guardian-v9-build` found:

- Every copied source file is present in the recorded source manifest, and every
  snapshot hash matches both that manifest and its current original. The adapter
  input hashes also match, with its unchanged-input result and exit 1 preserved.
- Both selected guardian executable dep-info files together name exactly the 16
  project-owned files in the full export. No compiled project source is absent.
  The test executable explicitly compiles `tests/links.rs` and
  `tests/public_boundaries.rs` through `#[path]` modules. These are legitimate
  members of the combined production/unit measurement population. The driver's
  `src/*.rs` plus protocol expectation omitted both.
- The only remaining exported filenames are the pinned standard library's
  `alloc/src/macros.rs`, `core/src/macros/mod.rs`, and `core/src/panic.rs`. Their
  file-summary denominators are all zero. They still carry 6, 230, and 24 code
  regions respectively in guardian-owned function mappings, reflecting the
  V9 macro-definition aliases. Zero file-summary totals do not justify deleting
  their mappings or ignoring their ownership provenance.
- The Linux discovery source is absent from the compiled dep-info and the Darwin
  export, matching its platform condition. It remains explicitly unmeasured.

The unit run reports 17 passing tests. Actual-protocol execution, merge, report,
and export each exited zero. The full unfiltered export reports 1,430/1,682
lines, 113/121 functions, and 3,438/4,476 regions, with zero saturated function
regions. Included test modules account for 204/204 lines, 12/12 functions, and
647/789 regions; they remain part of these full totals. This is diagnostic data,
not an accepted gap inventory or a full coverage pass.

The immediate rejection belongs to the coverage driver's stale ownership
expectation, not missing project source or unexplained test source. A correction
should derive the project inventory from the actual compiled executable dep-info
and reconcile supplemental macro aliases with their owning functions and pinned
compiler source provenance. It must preserve the complete export and reject
unexplained additions or missing compiled files. Because the current driver exits
before producing its final binary/profile manifest and counter-sanity artifact,
those normal final artifacts are absent; this audit does not retrospectively
claim the original driver completed its acceptance sequence. No build, test, or
fixture execution was performed by this reviewer. General compiler mapping
acceptance remains blocked by the separately recorded V9 findings.

## Expansion/lowering lineage bridge: preliminary architecture review

Reviewed the ignored `expansion-baseline`, `lowering-baseline`, and
`lineage-baseline-driver` source and the pinned compiler's expansion/lowering
contracts. This is a prospective review of extension points; final V10 lineage
capture was not frozen and is not accepted here. No build or execution was run.

The completed `lineage-baseline/proof` six-case comparison was independently
checked from raw artifacts: macro/async, inlining at optimization zero and three,
cross-file macros, lazy entry, and identity macro cases have identical executable
hashes, complete JSON exports, stdout, and LLVM IR after removing only ModuleID
comments between the prior statement adapter and the unchanged bridge. Recorded
input hashes and both bridge source/dependency manifests match. The proof's
input list omits the lazy/identity fixture hashes and complete transitive linked
inputs; the author was asked to preserve the original record and supply an
explicit provenance addendum. Behavioral parity of these bridge samples does
not establish correctness of newly captured lineage.

The following contracts must govern the future capture implementation:

- `ResolverExpand::visit_ast_fragment_with_placeholders` runs after nested macro
  calls have become placeholder NodeIds. The final `PlaceholderExpander` replaces
  them later. An expression fragment root can therefore point to a further
  expansion rather than its final AST expression. Preserve and resolve exact
  expansion/placeholder edges; source containment cannot substitute for them.
- `lower_expr_mut` has early returns for parentheses, for loops, and closures.
  Observe its actual returned HIR expression outside the stack-growth closure,
  without changing the expression. Parentheses reuse the inner HirId; the map
  legitimately has multiple AST origins for one returned HIR identity. For-loop
  lowering explicitly warns that its returned HirId differs from `e.id`'s ID.
- Store full HirIds, including owner, and keep AST/expansion identities scoped to
  one compiler session. Item-local IDs alone collide across owners. Session
  teardown or an explicit single-session invariant must prevent stale global
  entries from being reused. Do not retain borrowed AST references after
  `lower_to_hir` drops its AST owner.
- The pinned `resolver_for_lowering_raw` and `lower_to_hir` queries are both
  `eval_always`; neither declaration promises a persisted lineage side table.
  Capture must still precede consumption for each actually requested owner,
  tolerate normal in-session query reuse, and fail closed on missing required
  lineage. A cold/warm incremental comparison remains needed for implementation.
- Keep the registry in one shared dependency instance across bridge libraries.
  Independently compiled duplicate statics would silently split producers and
  consumers. Acquire locks only around completed record insertion/lookup; never
  hold a registry guard across resolver forwarding, recursive lowering, compiler
  queries, or callback invocation. Preserve every ResolverExpand method's
  original receiver, arguments, returned values, and ordering.

These are identified implementation constraints and validation requirements,
not claims that the unfinished lineage implementation already violates them.
A completed frozen patch, nested/forwarded/lazy-origin oracles, artifact identity,
and independent re-review remain required before acceptance.

## Frozen V10 quality review: typed lineage and validation gap

Reviewed wrapper `ec3ac708e7d483784f22d54c3b8ab4c4edc95ff35435cf1ae657ec2b57c8bb07`,
the expansion registry/resolver proxy, AST lowerer wrapper, THIR bridge and final
collection. Independently rehashed all 16 frozen-candidate entries and all 44,009
outer collection inputs; before/after records are identical and current hashes
match. This reviewer performed no build, test or new profile collection.

The new registry records typed original AST roots and placeholder edges, resolves
chains with missing-target/cycle assertions, preserves separate expansion IDs,
and indexes returned full HirIds rather than item-local IDs. The lowerer wrapper
observes the returned expression outside its early-return closure, covering
parentheses, loops and closures without rewriting nodes. Multiple AST roots may
map to one HIR identity. The resolver proxy forwards the full pinned trait
surface, recording only after the original fragment callback. Registry mutexes
are released before compiler queries, recursion, forwarding and provenance I/O.
One shared expansion library supplies the registry to both producer and consumer
libraries. The executable begins one session, clears it on successful completion,
and does not support embedding repeated compiler sessions in one process.
Fatal process termination cannot feed stale state into a later wrapper process.

The exact HIR-to-THIR bridge requires one Scope/typed-constructor anchor per
captured expansion. Source context/body containment restricts ownership after
identity matching; it does not replace identity selection. Existing structural
entries remain separate and unsupported/ambiguous shapes fail closed. No new
P1/P2 was found in these inspected identity, forwarding or lock boundaries.
The whole-space completeness of this mapping still needs the canonical run and
valid footprint evidence below.

There is no raw-argv instrumentation bypass in this frozen driver. Every
invocation reaches rustc configuration; `config.opts.incremental.is_some()` is
checked before query overrides. Response-file flags therefore cannot bypass the
guard. Both direct and response-file rejection evidence show exit 2 with no
output/cache. The guard intentionally rejects even noncoverage incremental
requests: its contract is a fresh-compilation-only wrapper, not a selective
instrumentation wrapper. Full collectors explicitly use `CARGO_INCREMENTAL=0`.

Independent inspection confirms passing identity, cross-file and lazy validators
and the four extended macro/Tokio/inlining validators. Independently comparing
raw MIR while removing only Coverage statements confirms equality for 48
ordinary, 18 Tokio and seven identity bodies. Four actual cases report no
saturation, and the pinned syn compilation succeeds. These observations close
the demonstrated prior identity-initializer and syn compile failures for the
bounded cases; they do not close the following validation problem.

### P2: footprint comparison reports loss but cannot fail

`compare_footprints.py` compares current actual exports with old
`direct-counters/actual-stock` exports, then unconditionally writes/prints its
result and exits zero. The final collection log reports 1,113 missing functions
for both reader cases, 1,133 for context panic, and 1,121 for delayed EOF, with
corresponding removed footprint groups. Nevertheless `V10-EVIDENCE.md` calls the
footprint comparison a pass and the outer ten-command status treats exit zero
as successful validation.

Different source/configuration/symbol populations may explain these differences;
this is not yet a demonstrated V10 compiler mapping loss. It is a demonstrated
failure of the claimed preservation evidence. Before acceptance, collect a stock
comparison bound to the same current sources/configuration, or supply an
independently justified stable identity/source comparison, and assert that
required mappings are preserved. Retain the mismatch report and correct the
PASS wording. The author and parent were notified; fix and independent re-review
are pending. The canonical coverage run may continue as diagnostic evidence,
but its completion alone does not repair this P2.

## V10 canonical matrix: new compile failure

The subsequent `full-v10` canonical run completed in 50.519973993 seconds with
`passed: false`, unchanged production sources, and all original 100%/zero
uncovered thresholds retained. The reviewer independently read the phase records
and raw workspace logs and rehashed all 44,020 canonical adapter input entries;
every current hash matches. This was read-only review, not a repeated run.

All three workspace feature phases fail while compiling Tokio at generated
`coverage/mod.rs:165`, where `integrate_builder_source_points` calls
`graph.bcb_from_bb(bb).expect("source point is unreachable before mapping")`.
The workspace report consequently cannot qualify. Helper tests exit zero, while
its original threshold report exits one: 476/1,478 lines, 62/109 functions, and
1,143/3,687 regions. These partial figures are retained as reported diagnostics,
not accepted gap or readiness evidence.

**New P2:** the source-point integration assumes every retained typed marker has
a coverage-graph owner. The real feature matrix violates that assumption and
cannot compile. This remains open alongside the earlier footprint-validation
P2; the author's V11 repair is not yet reviewed or accepted.

A future fix must distinguish coverage-graph absence from proven nonexecution.
The pinned graph builds a filtered coverage subgraph and explicitly excludes
blocks ending in `Unreachable`; absence from that graph alone does not establish
that a statement cannot execute in the executable MIR. Cleanup/unwind handling
also requires explicit classification. A zero-valued mapping therefore needs
source-point identity plus a justified execution/optimization proof. Preserve
all source mapping obligations and denominators, record why each point receives
its owner or zero representation, and fail closed on unresolved cases. Do not
silently skip points, redirect them to a nearby live block, invent CFG edges or
increment sites, or weaken executable-MIR preservation. This constraint was sent
to the author before review of any proposed repair.

## Paired V10 footprint re-review: validation P2 closed in bounded scope

Reviewed `collect_paired_stock.py`, `compare_paired_footprints.py`, paired stock
metadata/artifacts and all four complete paired exports. The copied old checker
and misleading mismatch report remain preserved. `V10-EVIDENCE.md` now explicitly
retracts its original footprint-pass claim. The new collector requires the exact
current direct-run source manifest, flags and helper image and explicitly sets
`CARGO_INCREMENTAL=0`; independent metadata comparison confirms equality of
those sources/flags/helper hashes. All ten recorded stock commands exit zero,
and every selected stock artifact matches its recorded hash.

The new comparator retains full raw symbols, physical filenames and region kinds.
It rejects duplicate raw function identities and returns nonzero when any old
function or any mapped interval is missing. It neither normalizes symbols nor
pools unrelated functions. The reviewer independently recomputed interval
coverage from the raw exports with a separate traversal and found zero lost
intervals in every case.

| Case | Raw functions, stock/direct | Lost functions/footprints | Added footprint groups | Function/line denominators, both arms |
| --- | --- | --- | --- | --- |
| Reader panic | 1,534 / 1,534 | 0 / 0 | 407 | 1,216 / 9,952 |
| Reader normal | 1,534 / 1,534 | 0 / 0 | 407 | 1,216 / 9,952 |
| Context panic | 1,138 / 1,138 | 0 / 0 | 592 | 1,001 / 9,731 |
| Delayed EOF | 1,542 / 1,542 | 0 / 0 | 409 | 1,219 / 9,943 |

These additions represent extra mapped interval groups, not missing mappings
being ignored or a claim that each new count is semantically proven. This
re-review closes the P2 for the declared four-case source-footprint preservation
claim. It does not establish preservation for the entire crate ecosystem or
repair V10's separate canonical Tokio compile failure. No build, test or profile
collection was run by this reviewer.

For the proposed V11 zero representation, the pinned stock counter code does
reserve the maximum BCB as the first priority-list entry and asserts it has no
observed virtual counter. That representation can be considered only after
independent operational reachability proves each affected source point cannot
execute. Reachability must include real unwind/cleanup and Yield-drop successors,
exclude only explicitly imaginary FalseEdge/FalseUnwind edges, and must not skip
blocks merely because their terminator is `Unreachable`. Assert the actual
reserved sink identity, absence of increment and branch ownership, and final
`CovTerm::Zero`, while preserving each source point/span separately. These are
prospective review constraints, not acceptance of the unfinished V11 repair.

## Narrow V11 erased-marker proof review (before final fixture freeze)

Scope was limited to the two existing focused fixture failures; no additional
oracle or prototype was requested or executed. The new raw-marker proof was
reviewed against pinned `lint_and_remove_uninhabited.rs` and
`ty/inhabitedness/inhabited_predicate.rs`.

`raw_points::capture` uses the stock destination type, parent module and body
`TypingEnv` predicate to exclude a call's normal return edge only when
`is_inhabited_from` is false. The pinned predicate treats unresolved generic
projections, unknown opaque types, unknown constant lengths and normalization
cycles conservatively as inhabited. Those cases retain their return edge; they
cannot acquire a zero proof merely because information is unknown. A proven
uninhabited call retains its cleanup unwind successor. Other real successors,
including coroutine drop, remain in the traversal; only explicit imaginary
FalseEdge/FalseUnwind alternatives are excluded.

Every source point must have exactly one original marker with its recorded ID,
span, raw block and statement index, and total point/marker inventories must
agree before storing the proof. A marker erased by later cleanup can use the
zero owner only by looking up that exact original point's proven-unreachable
receipt. Its source mapping is still appended, and the source union assertions
remain. No new issue was found in this narrow predicate/identity review.

This is source review only. The eager metadata relationship and final frozen
focused outcomes remain pending; no previously reported failure or canonical
compile blocker is closed by this entry.

The subsequent narrow eager-receipt source review confirms the proxy preserves
the stock resolver result and records only successful resolution. The pinned
expander supplies the invocation's own ID for monotonic expansion and the
original root ID for eager expansion; the adapter records only differing IDs.
Thus the parent relation comes from the compiler callback, not source-coordinate
containment. An eager metadata receipt requires an explicit ancestor expression
root consumed by the typed attribute parser. The independent fixture validator
must reconcile these distinct eager receipts and their parent chain without
weakening its existing AST-root reconciliation or runtime count obligations.
No missing `stringify!` expectation may be waived; parser adaptation is being
reviewed separately before a new focused collection.

For the two concrete control-fixture source failures, the current repair keeps
both runtime expansion identities but exposes one physical invocation for an
exact single-placeholder forwarding relationship. Equal callsite bytes, direct
hygiene parentage and identical BCB ownership are required; independent repeated
expansions are not collapsed by coordinate equality alone. A proven-never-entered
macro may retain its exact complete callsite as zero, including argument bytes.
The existing source-union checks remain. This is narrow source inspection while
linked evidence is pending, not closure of the focused or canonical failures.

## Final frozen V11 bounded quality re-review

Verdict: **PASS for the bounded measurement-tool repair**, with no unresolved
P1/P2 quality finding in the reviewed changes. This does not pass the complete
acceptance gate or close the canonical matrix, coverage, or performance work.
The separate adversarial review and full canonical results remain required.

Reviewed the final exact-forwarding selection in
`builder-v11/source/builder/coverage_points.rs` and the typed alias ownership in
`expansion-v11/source/lineage.rs`, together with the previously reviewed raw
marker, inhabitedness, eager metadata and formatting-root fixes. For an exact
forwarding relationship, the builder replaces only that expansion's earlier
structural selection with its original HIR-to-THIR expression anchor. It asserts
one anchor and the nearest associated body owner; both parent and child retain
separate runtime identities. The same-entry assertion is preserved. This fixes
the recorded futures-channel parent bcb0/child bcb26 mismatch without using
observed counts or coordinate equality as execution identity. The registry owns
compiler lineage facts; the builder owns entry selection; metadata consumption
has its own terminal disposition. Those responsibilities and dependency
directions remain cohesive for the compiler adapter. No new abstraction or
pattern is needed for this narrow correction.

Independent read-only evidence audit:

- Rehashed all 178 entries in `direct-counters-v11/frozen-candidate.json`:
  no mismatch. Manifest SHA-256 is
  `a027a27c97a358d40e40284209e0574df088791161a2a4d344903941bc6c7c08`.
- All 27 focused inputs match before, after, and current disk contents. Both
  isolated provenance inventories exactly match all four JSONL files in each
  case, including current content hashes; no unlisted file was ignored.
- Saved focused validation passes with 244 control-flow and 555 formatting
  per-character source checks, no failures, and validator exit zero. Saved
  stdout contains the expected 16 control observations and 13 formatting
  observations. The approved validator remains
  `8518afc58d325534d5b504bdb1a70162e9c6e4b1e274786699b38fb9259a4804`.
- Independently compared the saved MIR text, removing only Coverage statement
  lines: all 48 ordinary, 18 Tokio and seven identity bodies agree. The saved
  initial-failure and repaired Tokio inventories each contain exactly 2,512
  matching body names, with zero executable-text differences by the same rule.
- Saved final-small-checks records cross-file, lazy-entry and identity passes;
  Tokio and syn compile records report exit zero and unchanged inputs. Saved
  paired stock footprint results report zero missing functions and zero removed
  intervals across the four actual cases. These remain bounded observations.

No compiler build, new oracle, benchmark, or production modification was run
for this re-review. Attempts 01–05 and their failures remain evidence; this
verdict supersedes only the source-only pending status of the narrow V11 fixes.
Warm incremental reuse is still unsupported and rejected. This review does not
infer caller cleanup/drop-marker placement solely from Drop-body observations,
or claim complete source coverage from the fixture sample. The canonical full
V11 run must stand on its own frozen manifest and unchanged original thresholds.

## V12 concrete forwarded-parent quality re-review

V11's complete canonical run failed in the release_load test-harness compile;
its bounded verdict above did not predict full acceptance. V12 addresses that
observed blocker only. Verdict: **PASS within this targeted source/evidence
scope**, with no new unresolved P1/P2 quality finding. Full canonical coverage,
mechanical/performance checks and the separate adversarial review remain gates.

The only semantic expansion delta recognizes `AstFragment::Stmts` containing
exactly one `StmtKind::Expr` or `Semi` as the enclosed original expression root.
The existing expression path then preserves its exact NodeId or follows its
placeholder expansion chain. A semicolon contributes no separate evaluation
entry. Empty, multiple, item, let and other statement fragments keep their prior
classification; no source-position or observed-count heuristic was introduced.
Expansion owns this typed classification. Independently compared lowering and
builder source trees: both are unchanged from V11. The driver delta emits a
missing-parent diagnostic before the same existing panic; parent-existence and
same-BCB assertions are not relaxed.

This makes the affected parent88 and child234 share original NodeId12625,
HIR local2303, THIR Expr(e2474), and the same nearest coroutine body. Distinct
markers225/226 both own bcb612. The parent placeholder chain is [88,234], the
child chain is [234]; exact callsite byte equality and direct hygiene parentage
still justify the source alias. Transitive traversal remains cycle-checked,
and the ultimate representative must still exist among this body's runtime
points with equal BCB ownership. No missing-parent fallback or omitted child
mapping is accepted. Root-origin provenance exposes the new typed statement
classification. The repair fits the existing ownership boundaries and adds no
new runtime work to production PTY code.

Read-only evidence verification independently rehashed all 287 entries of
`direct-counters-v12/frozen-candidate.json`, SHA-256
`c9514ff3678bd7d17e6456e9a56d15ba272a62cad79ca391587d7c1ff9e7b16d`,
with no mismatch. All 631 release-load bindings and 27 focused-fixture bindings
match before, after and current disk contents. The saved exact failed
compile/link rerun exits zero in 3.504 seconds. Independently comparing raw MIR,
removing only Coverage statement lines, finds all 53 old bodies present and
unchanged; the repaired compile emits another 166 bodies. Those added bodies
have no old completed counterpart and are not claimed as paired preservation.
Both unchanged focused fixtures pass the approved validator (244 and 555 source
checks), and their complete provenance inventories match current files/hashes.
The saved terminal receipt reports 261 mapped runtime roots and zero unsupported
roots for the repaired command. This is compile and fixture evidence, not proof
that the full release-load test harness executed.

No build, new case, prototype, benchmark or production edit was performed for
this review. V11 failed artifacts and the 53-body partial baseline remain intact.

## Completed V12 canonical matrix: measurement audit

The full V12 run completed in 207.235 seconds. All workspace all-targets phases
(all features, no default features, and event-stream without defaults), helper
preparation and separate helper tests exited zero. Both reports exited one
because the original 100% line/function/region and zero-uncovered thresholds
remain enforced. Overall acceptance is **FAIL**, despite successful execution.

| Report | Lines covered/total | Functions covered/total | Regions covered/total |
| --- | ---: | ---: | ---: |
| Workspace | 9,965 / 10,535 | 1,029 / 1,104 | 23,402 / 27,028 |
| Guardian helper | 476 / 1,478 | 62 / 109 | 1,143 / 3,687 |

Independently read the canonical phase commands/exit codes and recomputed these
totals from `full-v12/workspace.json` and `helper.json`. Rehashed all 45,710
measurement-adapter tool/source inputs against current disk: zero mismatches;
all 351 canonical source-file hashes also match. Saved run metadata reports
source and tool inputs unchanged across collection. Examined all 785,804
workspace and 8,306 helper exported function-region count entries: none reaches
2^63, so this audit found no saturated or wrapped sentinel counts. This check
alone does not establish correctness of every dynamic count.

The completed matrix extends the bounded compiler repairs to these real build
and test phases: the prior unsupported-dependency, erased-marker and
forwarded-parent compile failures did not recur. It does not make the previous
fixture proofs universal or erase earlier failures. Coverage gaps must be
triaged by actual source behavior and source-region union across applicable
instances; a zero-count unused monomorphization is not by itself a missing
public test. Raw export function inventories include far more instantiations
than the source-function report and must not be substituted for its denominator.

The helper report is not evidence that all unreported process behavior was
unexercised: abrupt process termination and profile persistence remain a
measurement integration concern. Existing independently reviewed continuous
profile evidence must be integrated into the maintained runner before claiming
that loss resolved. New application/domain tests must assert public contracts
such as redaction and journal behavior; private poison/overflow state injection
or assertions mirroring implementation solely to fill counters are not an
acceptable replacement. No additional build or benchmark was run in this audit.
