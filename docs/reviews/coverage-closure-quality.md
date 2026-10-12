# Coverage closure: independent code-quality and measurement review

## Result and identity

Code-quality review: PASS for the inspected test/runner changes, with no scoped
P1/P2 implementation finding. Readiness remains BLOCKED: this is not a 100%
coverage acceptance. Final re-review is required after the source freeze and
final mechanical/coverage results.

The reviewed inventory is `work/coverage-closure/quality-review/source-manifest.json`,
produced by `scripts/record_validation.py::source_manifest`. SHA-256 of that
exact JSON file is
`18c4a166780c6c741c226f3b2bd1f134148acdc2674c2eae514e3e563611b6b7`.
This is a review-time inventory, not an assertion that the concurrent authoring
work had already frozen. The diff adds tests across domain/application,
infrastructure, guardian and facade; production Rust edits only register test
modules under `cfg(test)`. Native test shim changes remain separate from shipped
bridge objects. The native coverage driver adds four real native contract targets.

## Design and test assessment

The tests keep ownership and dependency direction intact. Domain public tests
exercise validated bounds and state preservation. Application tests use existing
external ports for injected adapter failures and assert admission release,
observer results and lifetime behavior. Runtime poison/exhaustion cases synthesize
otherwise impractical internal states locally without exporting a production
constructor. These are boundary tests, not evidence of actual platform failure
rates. Files remain cohesive and no generic testing framework or runtime feature
was introduced solely to obtain a percentage.

Infrastructure checkpoint tests use real private temporary directories and real
protection/storage adapters, asserting quota and file ownership after rejection.
Guardian Links tests use actual socket pairs and separately check duplicate
suppression, queued delivery and endpoint isolation. Startup tests execute the
actual helper binary and bound their waits with cleanup on failure. Preserving
`LLVM_PROFILE_FILE` in these invalid-CLI tests does not weaken the separate Empty
workload environment contract; no admitted workload is started by those cases.

The own-C shim covers the adapter's response to explicit native return values,
including pending/complete compression and decoder budget failure. That is honest
conversion-contract evidence, not a claim that the real native engine naturally
produced those failures. The coverage driver records synthetic fault runs
separately and also runs real terminal contract binaries.

## Coverage findings and remaining blockers

P2 readiness blocker: the existing `scripts/coverage.py` helper phase primarily
measures its own unit/CLI tests while the workspace runtime matrix intentionally
uses an uninstrumented helper. The old `coverage-final` result therefore contains
only 201/1478 helper lines, 21/109 functions and 365/2274 regions covered. These
are honest scoped numbers, not evidence that actual guardian behavior was
measured. New tests require fresh measurement; they do not justify estimating
updated percentages.

P2 readiness blocker: the actual-helper continuous-profile experiment remains
invalid. All 20 real protocol scenarios passed, but 50 of 120 individual profiles
contain saturated derived region counts; the combined export contains ten such
regions. The collection must not be accepted, clamped, or repaired by omitting
those production regions. Raw evidence is retained under
`work/coverage-closure/guardian-fork-evidence`, particularly
`individual-sanity.json` and `export.stdout.txt`.

The experiment changed no production source. A naive shared continuous mapping
first reproduced a false zero on an executed parent path. A diagnostic-only
atfork hook then used LLVM runtime APIs to assign child profiles independently,
preserving pre-fork counters. Tiny C/Rust nested-fork fixtures correctly recorded
parent/child paths, `_exit`, SIGKILL and a never-called function. Shared prefix
counts are duplicated per process, so even the successful tiny fixture supports
boolean coverage rather than global execution-frequency claims. The real helper
still failed counter sanity: snapshots of incomplete caller/control-flow
expressions and abrupt process lifetime require further toolchain investigation.
The tiny fixture is insufficient validation for accepting the helper's numbers.

The hook refuses threaded callers and unsupported bitmap collection. It is only
a rejected experiment for fresh, single-threaded helper/fixture processes, never
a permissible hook in the multithreaded runtime host. No hook integration or
production-only coverage alteration is approved by this review.

Scope blocker: Rust coverage alone cannot establish own-C, Linux-only code or
other platform coverage. The own-C driver must be run with `--require-complete`
and its explicit four-file denominator checked. Linux discovery and other
conditional platform paths need executed evidence on those platforms. The current
runtime CI workflow does not enforce the additional coverage command; a green
runtime gate is not a 100% coverage result. Branch data with count zero means
unmeasured, not 100%.

## Evidence and acceptance route

Inspected the actual test source and available helper/native/composition test
logs. The experiment independently executed the actual helper's protocol probe;
its functional success is separate from its rejected profiler evidence. No
mechanical pass or numerical threshold pass is inferred from test inspection.

Keep the current denominator intact. First finish and freeze the test additions,
run the full mechanical gate, then run fresh isolated Rust/helper and own-C
coverage and preserve the source manifests and exact uncovered counts. Review
remaining regions by contract, adding behavior assertions through existing
public APIs or external boundaries. The fork collector must independently prove
correctness on a representative reduced control-flow example before retrying
actual helper collection. A trusted 100% result for that helper and executed
platform evidence are still required; this review cannot promise exact 100%
while those collection and execution gaps remain.

The available own-C completion artifact was also inspected:
`work/coverage-closure/native-complete-evidence/coverage-report.stdout.txt`
reports 249/249 lines, 23/23 functions, 356/356 regions and 198/198 branch sides.
All nine recorded native/allocator/boundary executables exited successfully, and
the boundary run is explicitly marked synthetic. This is complete coverage for
that four-file own-C denominator, not guardian, upstream Zig or other-platform
coverage. The broader scope blockers above remain.

## Final corrected-source re-review

Scoped code-quality PASS at the final reviewed inventory
`work/coverage-closure/quality-review/source-manifest-final.json`, SHA-256
`2bcbd8bfe1b467c28cb3225bbb618df7e230c1a474d4f7b91be9976e36b0ae84`.
The base revision is `5c7434ac2c129870dc6b1adbfa9a5d493b444cdd`.
No remaining scoped P1/P2 implementation finding was identified. Coverage
readiness remains BLOCKED pending the distinct measurement requirements above.

Correction to this review's initial assessment: the DDD reviewer subsequently
identified two P2 findings that the initial quality pass missed. The first
version's private counter/poison manipulation did not satisfy the required
public-surface testing method; describing the setup as synthetic was not enough
to accept it. The original descriptor test also inherited an already-set
FD_CLOEXEC flag and did not prove adoption established it. Those parts of the
initial assessment are superseded by this correction and the re-review below.

Inspected the actual revised code: `runtime/lifecycle_tests.rs` is absent,
`runtime/lifecycle.rs` is identical to HEAD, and the runtime boundary additions
no longer manufacture counter exhaustion or directly poison private mutexes.
The new `owner_boundary_tests.rs` uses public Runtime and Session operations
with existing repository/process ports. Observer refusal and reacquisition are
checked through reported shared resource usage. Shutdown uses bounded channels
to hold an admitted external spawn, observes shutdown, verifies new admission
is refused and verifies cancellation of the returned process. It adds no
production fault setter or visibility widening. These checks support application
orchestration, not real process or native behavior.

The descriptor test now retains socket ownership while clearing FD_CLOEXEC and
asserting that precondition, then transfers ownership into adoption and checks
the set flag and peer EOF after drop. Inspected `cloexec-mutant-final.log`: the
isolated F_GETFD mutation fails the missing-flag assertion. Inspected
`cloexec-correct-final.log`: the corrected test passes against production code.
The supplied `work/coverage-acceptance-followup/review-fix-tests.log` records all
150 application tests passing. The independent DDD re-review also records its
own affected reruns. This quality re-review ran no tests/builds, avoiding
interference with the frozen mechanical/performance run in progress.

The rejected helper-profile experiment remains unchanged and unintegrated. No
accepted helper coverage or aggregate 100% claim follows from these fixes.
