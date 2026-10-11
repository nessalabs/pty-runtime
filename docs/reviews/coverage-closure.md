# Coverage follow-up: scope, evidence, and remaining blockers

This follow-up retains the 100% target and zero-uncovered requirements. Both
program-status PRs remain drafts until that target and the required gates pass.
The batch adds 50 Rust behavior tests plus four native bridge contract cases;
production error paths and coverage thresholds are unchanged. A subsequent
three-line scanner optimization skips a redundant plain-text search only when
the scanner is in Ground state and the chunk contains no escape byte.

## Changes and evidence

- Domain/application tests cover validation, bounded admission, refusal rollback,
  projection construction, worker ownership, and shutdown through existing ports.
  These doubles establish orchestration, not real PTY/native behavior.
- Infrastructure tests use real encryption and filesystem storage, check cleanup
  refusal/recovery and foreign-content preservation, and check process/scheduler
  admission through their public interfaces.
- Guardian tests use real socket pairs for status delivery, deduplication,
  queue bounds, invalid frames, and one-sided failure. Fresh guarded child
  processes test CLI validation. Read-only discovery and descriptor contracts
  do not change the test host's session or signal dispositions.
- The native coverage driver now runs all seven existing terminal contract
  executables. Explicit test-only external-call faults cover pending/completed
  compression results and decoder-owned budget refusal; a real formatter call
  verifies successful output. These do not claim Ghostty naturally produces
  the injected fault cases.

The first frozen Rust measurement is preserved at
`work/coverage-closure/coverage-first`: all test phases passed, thresholds failed,
and sources were unchanged. Its workspace result was 9982/10566 lines,
1035/1107 functions, and 14721/15853 regions. Its helper result was 476/1478
lines, 62/109 functions, and 747/2274 regions. This includes subsequently
rejected private-state test fixtures and is **not final acceptance evidence**.

## Findings and fixes

The independent domain/boundary reviewer found two P2 problems:

1. Direct private counter writes and mutex poisoning manufactured states without
   the required public failure path. Those tests were removed. Observer refusal
   and shutdown now use public Runtime operations and existing external ports.
   Unreachable defensive paths remain gaps; no production seam was added.
2. A descriptor test inherited close-on-exec from socket construction. It now
   clears and verifies the precondition before adoption. An isolated mutation
   replacing SETFD with GETFD fails that assertion; unchanged production passes.

Both findings were independently re-reviewed as resolved. The code-quality,
adversarial, and domain/boundary reviews report no remaining P1/P2 code finding
in this batch. Their ledgers and author fix record preserve the original
findings, fixes, evidence, and limited scope. They do not waive coverage.

The shutdown test proves rejection of late admission and cancellation of a
late-bound process. It alone does not prove shutdown waited for every admitted
operation; a scoped-thread join is not such an assertion.

## Rejected guardian instrumentation experiment

Normal coverage measures the helper's unit/CLI tests separately from workspace
runtime tests, which use an uninstrumented helper to preserve environment
semantics. This omits integrated helper execution from the accepted counters.

The experiment in `work/coverage-closure/fork-proof` tested per-child continuous
profiles with LLVM runtime APIs. Simple C and Rust fork/_exit/SIGKILL fixtures
produced consistent Boolean coverage, with duplicated prefix counts explicitly
unsuitable as global execution frequencies. The real helper still failed the
counter sanity audit: 50 of 120 profiles had saturated derived regions, and the
combined report had ten saturated rows. All twenty protocol cases passed, but
that is behavior evidence, not permission to trust invalid derived coverage counts.
The saturated regions overlap active cleanup-loop tails; asynchronous termination
can leave inferred counter expressions inconsistent. This evidence does not
uniquely identify fork sharing as the cause.

The actual-helper report in `work/coverage-closure/guardian-fork-evidence` is
rejected. No hook was integrated, no bad regions were discarded or clamped,
and no profiler allocation was added to the fork-isolated materializer.

## Remaining scope

A percentage for own C does not qualify the Rust workspace, guardian, upstream
Zig engine, or other platform configurations. Rust branch/MC/DC instrumentation
reports zero records, which is unmeasured, not 100%. Inline test assertion arms
also occur in the existing Rust denominator. OS allocation/thread/fork/entropy
failures, poison recovery without a public inducing path, and forked `_exit`
paths remain explicit gaps; this batch does not manufacture invalid state or
remove production defenses to satisfy them. Linux coverage and full ADR
performance/lifecycle/reconnect/soak qualification remain outstanding.

Final mechanical, coverage, and controlled performance results follow below.

## Scanner cost investigation

The failed final-source runtime cohort is retained: one-session median regressed
16.70% and sixteen-session median regressed 34.59%, both beyond the unchanged 10%
limit. All byte, cleanup, and source-identity checks passed. Broad process samples
were dominated by waits and did not establish a causal bottleneck.

An isolated experiment compared the original domain crate against a scan-entry
fast return using its existing `plain_chunk` predicate. Five alternating process
pairs, each with five trials per existing parser workload, reduced ordinary-text
8 MiB run-median time from about 2.02 ms to 0.27 ms. Non-timing fields matched for
all paired workloads; 70 candidate domain tests passed. Continuation states never
use this shortcut. Raw data and copied sources are under
`work/coverage-closure/scanner-experiment`. This supports the narrow optimization,
not an explanation or waiver of the integrated performance failure.

A separate conventional-profile experiment used a linked compile-time `%p`
filename without continuous mode or a fork hook. Seventeen unit tests and all
20 real protocol cases passed, with no saturated counters. It produced no useful
actual-helper coverage: the sentinel also kills its own supervision group, so
normal profile flushing never occurs. Twenty helper-start profiles were empty;
nonempty actual profiles came from the workload fixture, not the helper. Unit-only
and merged helper exports were identical. This is not added to accepted coverage;
evidence remains in `work/coverage-closure/guardian-conventional-evidence`.

## Frozen final-source checks

The final candidate source digest is
`07ea3217667be0e547ec5fd403ef8175c234dc98832a3cca6facb3158c29c092`.
The mechanical gate passed in 132.604 seconds with an unchanged source manifest
(`work/coverage-closure/gate-scanner`). The separate independent quality review
of the final added tests is in `coverage-closure-independent-tests-quality.md`;
DDD and adversarial re-reviews also found no scoped P1/P2 code finding.

The strict coverage run failed in 117.823 seconds with unchanged sources
(`work/coverage-closure/coverage-scanner`). Every behavioral test phase passed;
only the two threshold-report phases failed. Workspace totals are 9955/10531
lines (94.53%), 1029/1104 functions (93.21%), and 14604/15729 regions (92.85%).
Helper totals are 476/1478 lines (32.21%), 62/109 functions (56.88%), and 747/2274
regions (32.85%). The workspace denominator includes some test scaffolding;
these are tool-scoped totals, not a pure production-code denominator. No
production error paths or thresholds were excluded or relaxed. The target
remains unmet, so neither PR is ready.

The final complete own-C measurement passed: 249/249 lines, 23/23 functions,
356/356 regions, and 198/198 branch sides. All nine executables passed (seven
real terminal contracts, allocator contract, and explicit synthetic boundary
contract). Evidence: `work/coverage-closure/native-scanner-complete-evidence`.
The immediately preceding invocation omitted the optional allocator-contract
argument and consequently failed at 99.20% lines/95.65% functions/98.60% regions/
98.99% branch sides. That incomplete invocation is preserved separately at
`native-scanner-evidence`; the corrected invocation includes the existing
allocator test, with no source or denominator changes.

The one prospectively declared scanner-candidate performance cohort also failed
(`work/coverage-closure/performance-scanner`). One-session baseline/current
medians were 48.390750/48.492667 ms (+0.21%, pass). Sixteen-session medians were
360.339000/434.551291 ms (+20.60%, fail against 10%). All twenty raw records met
exact byte, completion, stable-source, and zero-observed-descendant checks.
All samples and the prior failures remain recorded; no replacement cohort was
run. The scanner microbenchmark benefit does not explain this integrated failure.
Both the coverage and performance readiness blockers therefore remain open.
