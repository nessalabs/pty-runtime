# Guardian maintained collector quality review

Pre-collection verdict: **one P2 provenance finding pending correction**.
No build, collector run, probe or benchmark was launched by this reviewer.

The new `coverage_checks.py` is identical to the previously reviewed pure checks
apart from its module docstring. The collector keeps I/O/process orchestration
separate from source ownership and physical-counter validation. Direct functions
are appropriate; no new runtime hook or architecture layer is introduced.

The expanded population includes the helper binary, its unit test executable
and startup integration executable. Compiled source ownership comes from their
actual dep-info union, retaining test modules and startup source rather than
filtering denominators to production files. Installed-source macro aliases must
have nonempty code regions owned by a selected function. The source inventory
is exact, Linux discovery remains explicitly unmeasured, and 100% completeness
requires equal positive covered/total line, function and region counts.

Raw profiles are hashed before parsing; the sparse merged profile must equal
the sum of every raw physical counter by function name/hash identity, allowing
only omitted all-zero records. The export scan includes function, region,
segment and branch counts rather than checking function hashes as counters.
Final profile, snapshot and installed-source populations and hashes are checked,
addressing the earlier frozen-auditor population limitation in this integration.
The selected binaries and generated filename object are also rebound at the end.
These are necessary integrity checks, not universal dynamic-counter proof.

## P2: active validator and tool identity are not bound

The copied source manifest binds the requested `--source-root` snapshot, but
Python executes the original collector and imports its adjacent
`coverage_checks.py`. The latter is not hashed at all; the collector is hashed
only once for identity. With a different --source-root, the copied scripts may
not even be the executed scripts. Likewise version output identifies the
compiler/LLVM tools descriptively without binding their executable content or
the selected measurement wrapper before/after this collection.

Bind the actual executing collector and imported checker paths plus the tools
used by the run (or an explicit complete frozen tool manifest), retain that
binding in collector evidence, and verify it again before accepting readiness.
A parent-owned frozen V12 manifest can provide the compiler binding for this
run if the evidence explicitly links and validates it; copied project source
hashes alone cannot. The coordinator has been notified. This is a measurement
provenance requirement, not a request for a new compiler prototype or runtime
hook. Re-review is required after correction; final runtime evidence is still
pending and no accepted coverage result exists.

## Tool-binding correction and pre-collection re-review

The collector now binds the actual resolved `__file__` and imported
`coverage_checks.__file__`, sysroot rustc, the selected llvm-cov/llvm-profdata
executables, and resolved clang before compilation. When RUSTC_WRAPPER is set,
it requires that wrapper's sibling frozen manifest to contain the selected
wrapper, verifies every pinned entry, and incorporates both the manifest and its
complete dependency set into the run's tool manifest. The final integrity check
rehashes those actual paths along with profiles, binaries and installed sources
before writing readiness. This distinguishes executing scripts from the copied
--source-root snapshot and preserves the full compiler binding in the evidence.

Strict readiness additionally requires the measurement wrapper, rejects a
nested workspace wrapper, and forbids the optional fork hook. Source/profile
population checks and original positive 100% denominator checks remain intact.
For the planned run, the coordinating task selects the independently reviewed
V12 wrapper and frozen manifest; merely supplying some other manifest would not
constitute independent review of another compiler. Likewise this is an
initial/final content check, not a filesystem lock or proof against transient
changes restored between checks.

**P2 closed; pre-collection quality verdict PASS within reviewed scope.**
No additional P1/P2 source finding was found in this narrow correction. This is
permission to assess the resulting collection evidence, not a coverage pass.
The fresh serialized run, its complete 23-case probe results, compiler/tool
bindings, profile conservation and unchanged populations remain to be checked.
The reviewer ran no build, collector, test or benchmark.

Reviewed collector SHA-256: `61ea7c843a4720f50534d1a6bddefc52eab6c937af9cb675d24be7963b33f4d7`.
Reviewed pure-check module SHA-256: `c40190233e7af261a3d8f876a4c69d393aa86331259acb8e985be24301a7040f`.

## Completed collector03: independent post-run audit

**Collector implementation/evidence integrity PASS within scope; strict coverage
acceptance FAIL.** The saved readiness result is false: 1,581/1,739 lines,
124/127 functions and 3,952/4,716 regions. The original 100% requirement was not
lowered. All 17 helper unit tests, two startup tests and 23 protocol probes pass.
All 68 retained subprocess command records exit zero; the final collector exit
one is the strict readiness failure, not an ignored subprocess failure.

Reviewed the two corrections made after failed attempts 01 and 02. Cargo's
all-targets output includes a synthesized guardian-fixture harness even though
its target declaration has test=false. Requiring both profile.test and
target.test selects the two declared test targets: guardian units and startup.
The supporting fixture remains built, hashed and invoked by protocol probes;
it is not misrepresented as helper source coverage. Independently inspected
Cargo JSON: the two selected tests have both booleans true, while the extra
fixture harness has profile.test=true and target.test=false. This respects the
pre-existing Cargo target declaration and does not remove a declared test or
helper/unit/startup source from the denominator.

The profile commands now request --all-functions, --counts and --text for every
raw input and the merged output. The failed summary-only parse was not replaced
with a record waiver. Earlier failed artifacts remain evidence.

Read-only audit of `helper-continuous-current-evidence03` independently:

- Rehashed all 293 active tool inputs, five before-run binary/object entries,
  57 final binary/profile entries, 2,909 installed Rust-source files and 28
  copied project files; no mismatch. Snapshot and installed-source populations
  exactly match the manifests. The 53 current raw-profile paths exactly match
  both the final manifest and the actual merge command inputs.
- Parsed retained raw/merged text with an independent parser and recomputed
  180 raw function-name/hash identities, 170 merged identities, 10 omitted
  all-zero identities and 2,555 physical counters. Every merged counter equals
  the raw sum; the largest raw sum is 325,507.
- Re-derived the selected binaries' dep-info union: 17 compiled source owners
  match the exported function roots exactly. The 20 exported files comprise
  those sources plus three actual macro aliases: 236 core macro regions,
  29 alloc macro regions and 27 core panic regions, each nonempty code owned by
  a selected function. Test source remains included; Linux discovery is the
  declared platform omission.
- Independently walked 12,586 original exported count values across functions,
  regions, counted segments and branches; all are nonnegative integers below
  2^63-1. Maximum exported count is 330,929. No unsupported MC/DC data appeared.
- Recomputed the exact saved readiness totals and checked all 23 probe records
  report success. This audit does not convert missing source-region execution
  into harmless per-instantiation zeros, or claim universal process-profile
  completeness solely from arithmetic conservation.

No new P1/P2 finding was found in these corrections or this retained evidence.
The full project mechanical/performance gates remain separate; the measured
helper coverage requirement is explicitly still unmet. No build, collector,
probe or benchmark was run by this reviewer.

Post-run collector SHA-256: `1973d9ac691bd3dea6c328a6bd07d5b009351fbd98ddcbddf1a33b62944ecb24`.

## Canonical coverage wiring re-review

Reviewed the current `scripts/coverage.py` integration independently. The helper
phase invokes the strict continuous collector in fresh run-owned directories;
it does not skip the helper when workspace percentage reporting fails. The
collector's exact exit status is retained as a phase and participates in the
overall all-phases-success condition. Original workspace 100%/zero-uncovered
thresholds remain unchanged. Copying the retained export to helper.json preserves
the existing report location without declaring a failed collector successful;
consumers must honor metadata and helper readiness, because a raw export can be
retained even after a later integrity or threshold failure.

The canonical coordinator owns workspace/helper sequencing and overall status;
the helper collector owns continuous profiles and strict accounting; pure checks
own validation. This direction and division remain cohesive. No additional
framework is needed. No new P1/P2 finding was found; scoped source-review PASS.
Runtime evidence for this combined canonical command is still pending.

Operational limitation: the helper collector is validated only on Darwin and
strict mode requires the independently reviewed wrapper/frozen manifest. This
makes those prerequisites apply to the canonical command too. The planned
explicitly pinned Darwin run is in scope. An ordinary stock-compiler or Linux
invocation is not newly established as a supported complete coverage run; the
coordinator was asked to make that prerequisite clear rather than treating its
failure as a new uncovered-code result. No build was launched for this review.

Reviewed canonical runner SHA-256: `fcda021d77d332ee217c02a1524a5df2a5a32e004bf93db97ff41def311327f9`.

## Darwin workspace continuous-profile wiring

The subsequent canonical runner delta is source-reviewed PASS within scope.
It clears inherited LLVM_PROFILE_FILE_NAME and configures workspace compilation
with atomic profile updates plus page alignment for __llvm_prf_cnts, bits and
data. It selects `workspace-%p%c.profraw` through cargo-llvm-cov's supported
LLVM_PROFILE_FILE_NAME interface; it adds no exit or atfork hook. The separately
prepared uninstrumented helper and strict helper collector retain their distinct
lifetime and profile configuration.

Independently read the retained cargo-llvm-cov 0.6.16 source: the override accepts
a .profraw filename and joins it under the coverage target directory; configured
Rust flags feed the normal instrumentation setup. This avoids inventing an
unsupported environment override. The canonical docstring now states its
reviewed-wrapper/Darwin continuous-profile prerequisite, and metadata labels the
workspace continuous mode and helper units/startup/protocol scope.

No new P1/P2 finding was found in the addition. Its first complete matrix is
still running at this entry; no profile-capture, conservation or coverage pass
is inferred from source inspection. Existing continuous-profile oracle evidence
supports the mechanism, while the actual whole-matrix results remain necessary.
Only review documents were edited; the frozen source inventory is unchanged.

Runner SHA-256 at this review: `3666bc519848dde2b50d05a4be20fd0e6d99b29542f4c37fbfbbceb8aec6b79b`.

## Completed continuous workspace/helper matrix01

The combined canonical run completed in 210.307 seconds. All three workspace
test phases passed; workspace reporting and strict helper completeness failed
only their unchanged percentage requirements. Overall coverage acceptance stays
**FAIL**. This run reports workspace 10,045/10,529 lines, 1,035/1,097 functions
and 23,572/27,018 regions. Its helper result is 1,580/1,739 lines, 124/127
functions and 3,951/4,716 regions; do not substitute the preceding collector03
values for this new run.

Independent read-only audit of `full-current-continuous01` verified all 353
canonical source hashes and 45,710 operator-bound tool/source hashes against
current contents, with no mismatches at audit time. Metadata reports source and
tool identities unchanged across collection. Independently walked the complete
workspace export: 859,888 function/region/segment/branch count values are valid
nonnegative integers below 2^63-1, maximum 92,260,214; no unsupported MC/DC data
appeared. This is necessary saturation evidence, not proof of exact counter
semantics. The coordinator's separate raw physical-conservation audit is pending.

The nested helper's 293 tool, five binary-before, 57 binary/profile and 2,909
installed-source hashes match. Its 28 snapshot files match both contents and
population; all 53 raw profiles match the recorded population. The helper.json
copy exactly equals the nested raw export. All helper unit/startup/probe phases
pass, with 23 probe records; the collector log ends with the strict 100% failure.
Saved physical and export checks report conservation and no saturation, retaining
180/170 raw/merged identities, 10 all-zero omitted identities and 2,555 physical
counters. These nested checks do not imply workspace physical conservation.

### Observed materializer recovery

The production image_materialize.rs hash is identical in prior full-v12 and
this run: `164e85e4fce1b941495a7c2f789e01ca794ef71202cd45a5f7b835458201f0de`.
Its source summary improves from 85 to 141 covered lines of the same 189,
6 to 10 covered functions of 12, and 148 to 254 covered regions of 407.
This is recovery of recorded child execution, not a new materializer behavior
fix. Previously all six exported child_write and open_exclusive instances had
zero counts; current original records have [42,58,67,8,8,8] for each. errno now
has [0,0,0,4,4,4] and failure [0,0,0,2,2,2]. These per-instance counts are shown
as evidence, not summed to claim a source-wide invocation total. The child
executes _exit, so ordinary exit-based profile flushing had missed this work.

Remaining materializer source coverage is still 48 lines, two functions and
153 regions. In particular all exported abandon instances remain zero; this
requires behavioral triage, not an assertion that every zero instantiation is
an independent test gap. Subsequent native/materializer tests belong to a new
source state and cannot be attributed to this frozen matrix. No 100% or release
readiness claim is made. No reviewer build or benchmark was launched.

## Workspace01 physical audit re-review

The ignored retrospective workspace auditor now handles records actually
encountered in the retained workspace profiles. Duplicate physical records are
summed only for identical function names, function hashes and counter-array
widths; different widths fail. Names and hashes are not normalized. This follows
LLVM merge identity semantics and does not excuse a count mismatch. Nonnegative
integer arrays and arbitrary-precision sums cannot hide overflow or saturation
through cancellation. The auditor retains duplicate occurrences for inspection.

Decoded empty IR/Front-end profiles are accepted only after a successful LLVM
summary identifies zero functions, maximum function/block counts, block total
and total count. Unsupported headers/layouts still fail. These empty raw files
stay in the bound population. Final raw-profile membership is re-enumerated,
closing the earlier integration observation; original failed attempts remain.

Independent review rehashed all 360 bound inputs with no mismatch and confirmed
that the 355 raw paths exactly match disk. All 60 retained empty summaries were
parsed independently with exact numeric zero checks. The reviewer then ran
read-only LLVM profile dumps and an independent parser across every raw profile
and the merged profile, without invoking a build or executing test workloads.
Recomputed results agree: 42,514 raw identities, 15,493 merged identities,
27,021 omitted all-zero identities and 305,099 physical counters. Every merged
array exactly equals its raw sum. There are 266,831 duplicate raw record
occurrences (145 profiles, 2,523 distinct name/hash/width identities) and no
duplicate merged records. Largest summed physical counter is 92,260,214, equal
to the previously audited maximum exported count.

**Scoped retrospective audit PASS; no new P1/P2 finding.** This closes the
pending physical-conservation evidence for workspace01, not its failed coverage
thresholds. It checks the records that exist and cannot alone establish that
every possible process produced a profile. The helper's stricter production
parser is unchanged; this is a bounded workspace audit, not a new compiler
prototype or a general profile-format parser.

## Final current continuous matrix02

The final test-source matrix completed in 212.227 seconds. All three workspace
functional phases passed; strict workspace and helper coverage still fail.
Workspace totals are 10,051/10,529 lines, 1,035/1,097 functions and
23,579/27,018 regions. Helper totals are 1,580/1,739 lines, 124/127 functions
and 3,945/4,716 regions. No preceding run's totals are substituted for these.

Independently rehashed all 353 canonical source entries, 45,710 measurement
inputs and 360 workspace physical-audit inputs: no mismatch. The 355 raw-profile
paths exactly match the audit's on-disk population. Independently walked the
complete export: 860,823 count values, none negative/noninteger/saturated,
maximum 92,260,214. The physical auditor differs from the reviewed workspace01
version only in its input/output directory. Its preserved result reports exact
raw/merged conservation for 42,524 raw and 15,503 merged identities, 27,021
all-zero omitted identities and 305,505 counters, with no saturation. All 60
empty summaries again have exact numeric zero statistics and known IR/Front-end
kind. The reviewer validated this run's bound audit implementation and result;
the complete independent raw-array recomputation recorded above was for run01.

Nested helper tool/binary/profile/installed-source manifests (293, 5, 57 and
2,909 entries) match current contents. Its 28-file snapshot population/hash and
53-profile population are exact; all 68 nested subprocess records exit zero,
including 23 protocol cases. The helper export copied to helper.json is byte
identical. The nested physical result reports 180 raw/170 merged identities,
10 all-zero omitted identities and 2,555 counters with maximum raw sum 322,361;
only the explicit 100% readiness requirement fails.

**Final scoped measurement quality/integrity PASS; coverage acceptance FAIL.**
The mechanical gate's separate completion cannot waive these remaining
coverage requirements. No source edits, builds or duplicate benchmarks were
performed by this reviewer. Subsequent performance review uses the gate's exact
existing outputs on the same 353-file source manifest.
