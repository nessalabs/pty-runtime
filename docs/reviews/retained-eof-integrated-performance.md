# Prospective performance qualification of the integrated EOF correction

**Collected-cohort verdict: PASS for the ordinary one- and sixteen-session
processing regression guard, after independent raw-evidence audit.** The plan
and pre-execution findings below are retained as history. This result does not
close coverage, coverage-integrity, hosted-CI or full ADR readiness requirements.

This plan is declared before collecting performance data for the authorized
actual-checkout EOF correction. The preceding scanner-candidate cohort remains
failed at sixteen sessions (+20.5951% versus the predeclared 10% limit), as
recorded in `coverage-closure-performance.md`. That failure is not waived.

## Frozen comparison and prerequisites

- Baseline: original worktree `../pty-runtime-program-status-baseline`, commit
  `2fca6559e95244f8b935ba30a71ef7241d23da09`, source digest
  `c93dbb7ba13f01a3dae26a42070b503e8129cdccdacac9b3a53a3bfb60ed0c1b`.
- Candidate: actual integrated checkout at base commit
  `a3e0fee3c5974e22486673d94cade302ad9b39ee`, source digest
  `e986497052ef14b8e5f1aeb65920a61ec7d9e2ea5942fb1b2e005a5b557210e0`.

Finish the actual mechanical, affected-contract and coverage runs before
measurement. Record their outcomes honestly; a completed but failing coverage
run is not a readiness pass. Freeze all source inputs. If a correctness fix
changes the candidate, update its declared identity before any trial rather
than measuring a different source under this plan silently.

Use the same physical host, toolchain and workload for both configurations.
Prepare both noninstrumented release executables before measurement with the
same verified native source/archive/library. Record exact runtime executable,
guardian and native artifact hashes, sizes, build features/settings and cleared
coverage-related environment. Assert the identity of each artifact class, not
merely that a combined artifact list is nonempty. Preserve preparation logs.

## One fixed cohort

The previous `run_scanner_controlled.py` method is suitable with the candidate
digest and a fresh output directory updated before execution. Run exactly five
pairs in fixed order B,C / C,B / B,C / C,B / B,C. Each existing release harness
run processes 4 MiB per session at one and sixteen sessions, using the unchanged
`scripts/performance.py` command and assertions. There is no overlap with builds,
tests, coverage, profiling or another benchmark.

Record actual order, before/after source identity, launch intervals,
platform/toolchain, available load/thermal observations, complete raw output and
all twenty measurement rows. Explicitly label unavailable environmental data.
No sample exclusion, replacement, early stopping, favorable pooling or retries
until green. A missing, invalid or timed-out trial prevents a successful complete
cohort; preserve that evidence rather than silently replacing it.

## Unchanged decision rule and scope

Candidate cohort median elapsed time divided by contemporaneous baseline median
must be at most **1.10 at both session counts**. Require every trial to pass
release-build and 4 MiB workload checks, exact raw/parser totals of 4,194,326
bytes at one session and 67,109,222 at sixteen, successful completion, no timeout
and zero observed descendants after shutdown. Independently inspect all records
and metadata. Record remaining descriptors and resource observations as well.
Publish all samples, ranges and within-pair ratios; pair ratios are diagnostic,
not an alternative statistic selected after seeing results.

The EOF correction concerns post-exit reader behavior. The harness's primary
elapsed interval ends after payload/parser completion while the producer remains
alive awaiting its exit trigger; it is therefore a regression guard for ordinary
processing, not a measurement of the defect's benefit. Preserve startup/cleanup
measurements and exact completion checks, but prove the EOF behavior with the
named functional regressions. Do not expect or claim that the correction itself
explains or repairs the prior 20.60% processing-time failure.

If the single cohort fails, keep performance acceptance blocked and investigate
the measured cause before proposing another qualification plan. If it passes,
report that scoped result alongside earlier failures and remaining variability;
it does not prove a sustained speedup, peak memory, tail latency, platform-wide
readiness or full ADR qualification. Coverage and hosted-CI acceptance remain
independent requirements. The reviewer will inspect results after collection;
no benchmark has been started as part of this plan or review.

## Historical prospective state: prepared driver, not executed

The ignored driver is `work/coverage-closure/run_eof_controlled.py`, SHA-256
`fa0ba83c2f60972d27c46f8344f95226d7616ce548c6f4fb7add3647b5c2fdb6`.
It declares fresh output `work/coverage-closure/performance-eof` and asserts the
baseline/candidate identities above before preparing release builds. Runtime and
guardian artifact lists must each be nonempty, and their identities plus the
shared native artifacts are checked around each trial. Exact byte totals and
zero observed descendants are enforced alongside the unchanged median rule.
Only Python syntax was checked; the driver has not been executed.

No second baseline is proposed for this acceptance cohort. A direct pre-EOF
versus post-EOF diagnostic would answer a separate causal question, but the
functional regression already supplies failure-before/pass-after evidence for
that behavior. Such a diagnostic must not replace the original-baseline guardrail
or be pooled into this fixed cohort.

## Historical prospective driver findings and revision before execution

Independent adversarial review identified two P2 issues in the unexecuted
`fa0ba83c…b5c2fdb6` driver:

1. Hashing only initially discovered artifacts did not bind timed Cargo's chosen
   executable to the prepared one; a newly selected runtime/helper path could
   escape that comparison while old files remained unchanged.
2. Missing/malformed evidence could throw before the remaining declared trials
   ran, and an incomplete dataset could crash summary creation instead of
   producing an explicit failed cohort.

The revised ignored driver SHA-256 is
`de8defd02def72690d55966776d3c583afd28dd2e475dded7a20e82f42bc24ac`.
At this prospective checkpoint it remained **unexecuted and pending independent
re-review**; the collected-cohort audit below supersedes that status. Preparation now
retains Cargo JSON messages and identifies exactly one selected test executable.
Every trial must report that exact executable in its Cargo `Running` line;
missing/multiple lines or compilation/checking during collection invalidate the
trial. Complete runtime/helper inventories and hashes are compared before and
after trials, detecting new or missing artifact paths as well as changed bytes.

Ordinary launch failure, missing output or malformed trial evidence is retained
with explicit per-trial reasons; collection continues through the remaining
fixed schedule without replacement. No median qualification is calculated for
an invalid or incomplete cohort. A source or artifact identity change is a
distinct safety stop: record an invalid-cohort abort and do not collect further
measurements from changed inputs. This stopping rule is declared before any
trial, not selected after results.

Lightweight synthetic validator checks covered executable matching, compilation
rejection, missing/malformed evidence and incomplete summaries; evidence is
`work/coverage-closure/eof-driver-synthetic-validation.log`. They ran no Cargo
command and created no measurement output directory. An initial synthetic
fixture needed its temporary path resolved through the macOS `/var` symlink to
match the actual preparation contract; that fixture correction required no
driver change. Independent re-review must resolve both P2 findings before the
cohort is authorized to run.

## Independent re-review and collected-cohort audit

Independent adversarial re-review closed both prospective driver P2 findings
before execution. The final driver hash was
`de8defd02def72690d55966776d3c583afd28dd2e475dded7a20e82f42bc24ac`
(11,052 bytes). The owner then ran the one declared cohort. Evidence is retained
under `work/coverage-closure/performance-eof`: `plan.json`, `execution.json`,
`summary.json`, preparation logs and Cargo JSON, `selected-executables.json`,
`build-artifacts.json`, and every pair's launcher log, metadata and raw rows.

The independent performance audit checked all twenty raw measurement rows,
recomputed the medians and ratios, and verified the fixed B,C / C,B / B,C / C,B /
B,C order. All ten launch intervals were nonoverlapping, with measurement
metadata timestamps inside their launch intervals. Preparation's Cargo JSON
selected exactly one runtime test executable per configuration; each timed
Cargo `Running` line selected that exact path. No trial compiled or checked
code. Complete executable/helper inventories, including newly appearing or
missing paths, matched before and after collection, and the runtime, guardian,
native inputs and driver were independently rehashed. Both source digests
remained equal to the frozen comparison above. No samples were excluded,
replaced, pooled with another cohort or retried.

Every row passed the release and 4 MiB/session checks, exact raw/parser totals
(4,194,326 bytes at one session; 67,109,222 at sixteen), successful completion,
no timeout, exit status zero and zero observed descendants after shutdown.
Every row retained five descriptors after shutdown.

## All samples and unchanged decision rule

Elapsed values are milliseconds. Ratios are candidate/baseline within the same
pair; these are diagnostics and do not replace the declared ratio of medians.

| Pair | Order | 1 baseline | 1 candidate | 1 ratio | 16 baseline | 16 candidate | 16 ratio |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | B,C | 41.378459 | 47.221333 | 1.141206 | 650.807333 | 479.758917 | 0.737175 |
| 1 | C,B | 47.400709 | 41.122458 | 0.867549 | 633.172958 | 616.754958 | 0.974070 |
| 2 | B,C | 45.533750 | 43.753959 | 0.960913 | 564.103875 | 730.795042 | 1.295497 |
| 3 | C,B | 41.540000 | 43.525417 | 1.047795 | 546.929959 | 367.702875 | 0.672303 |
| 4 | B,C | 47.457166 | 45.960834 | 0.968470 | 602.493208 | 442.027375 | 0.733664 |

| Sessions | Baseline median | Candidate median | Change | Maximum candidate median (1.10× baseline) | Verdict |
| --- | ---: | ---: | ---: | ---: | --- |
| 1 | 45.533750 | 43.753959 | −3.9087% | 50.087125 | PASS |
| 16 | 602.493208 | 479.758917 | −20.3711% | 662.742529 | PASS |

At one session the baseline range was 41.378459–47.457166 ms and the candidate
range was 41.122458–47.221333 ms. At sixteen the ranges were
546.929959–650.807333 ms and 367.702875–730.795042 ms respectively. One sixteen
session pair regressed 29.5497%; this does not change the predeclared median
decision, but demonstrates substantial variability.

## Environment, artifacts and resources

Both configurations ran on the same macOS 26.6 arm64 host with Rust 1.98.1
(`48a229cea`, 2026-09-01), noninstrumented release settings and the same native
inputs. The recorded one-minute load ranged from 8.3623046875 to
9.4541015625. The instrumentation-environment assertion passed. The thermal
probe reported no recorded thermal warning, performance warning or CPU power
status; actual temperature and throttling information were unavailable. This
must not be interpreted as proof that no throttling occurred.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| Baseline runtime executable | 3,511,856 | `0ae27124c54ecc384ffef2b207541719861e637ef158fe25ae8aa526d2513b1c` |
| Candidate runtime executable | 3,552,304 | `c081d10ea69818c1db94b2fa76d35ddd02493ef530d2242e0da015d853b05261` |
| Baseline guardian | 338,368 | `a9e9e852bd402cae45fcd08c03daf04dd8c20701dffa29c8b687284783b0195e` |
| Candidate guardian | 338,368 | `cbd3d46ec786bad998458660f248e658e13e1df46a38354f8505fbfbeda43901` |
| Shared native archive | 39,533,862 | `820d84e8cbc4be0ca26b9a1b71cfdc5befdc814c414bef7b0dbcf51069668c68` |
| Shared native library | 10,987,408 | `1679af9dcd92e55d154a1206c40d4bef7c0f148d222157e7a1a84d1bcec83ecd` |

Ready-state descriptors were 18 at one session and 153 at sixteen; observed
descendants were 3 and 48. Shutdown left five descriptors and zero observed
descendants in every run. RSS observations below are test-process point samples
after processing, excluding helpers/producers; they are not peak memory.

| Configuration | Sessions | RSS range (bytes) | Cleanup range (ns) |
| --- | ---: | ---: | ---: |
| Baseline | 1 | 11,370,496–11,452,416 | 912,666–1,016,500 |
| Candidate | 1 | 11,386,880–11,501,568 | 947,000–1,032,083 |
| Baseline | 16 | 138,117,120–155,090,944 | 1,332,541–2,098,792 |
| Candidate | 16 | 129,581,056–154,910,720 | 1,336,458–1,440,833 |

## Qualification limits and retained failures

This complete, valid cohort passes the unchanged ordinary-processing guard at
both session counts. Earlier failed cohorts remain failures, including the
preceding scanner candidate's +20.5951% sixteen-session result in
`coverage-closure-performance.md`; historical results in
`program-status-performance.md` also remain intact. The sixteen-session baseline
median moved substantially from the previous cohort's 360.339 ms to 602.493208
ms here. Together with the wide sample ranges, that change rules out treating
this pass as evidence of a sustained speedup or attributing the lower median to
the EOF correction. The measured processing interval precedes producer exit;
the EOF benefit is established by functional regressions, not this timing.

The actual integrated mechanical run passed. The integrated coverage run
completed but failed its coverage criterion; its export integrity also remains
under review. Those findings, independent code-quality/adversarial review
outcomes, and affected functional evidence belong to the separate integrated
quality ledger. This performance verdict supplies one required piece of the
quality gate and does not by itself declare the change or integration ready.
