# Coverage follow-up: prospective performance gate

Current verdict: **FAIL at sixteen sessions for the final scanner candidate**;
one session passes. The earlier coverage-follow-up cohort failed at both counts
and remains recorded below. Performance acceptance stays blocked under the
unchanged 10% median guardrail; earlier passing cohorts do not override it.

Before collecting the next cohort, the independent performance reviewer
confirmed this unchanged plan on 2026-10-11. This is a plan, not a result.

Use exactly five contemporaneous baseline/current pairs in this order:
B,C / C,B / B,C / C,B / B,C. Retain every trial. The unchanged release workload
projects 4 MiB per session with one and sixteen sessions. At both counts,
current cohort median / baseline cohort median must be at most 1.10. Every
trial must also satisfy exact output-byte accounting, successful completion,
no timeout, and zero observed descendants after cleanup. Paired ratios and
ranges are diagnostics; they do not replace the predeclared acceptance statistic.

Baseline remains commit `2fca6559e95244f8b935ba30a71ef7241d23da09`, source digest
`c93dbb7ba13f01a3dae26a42070b503e8129cdccdacac9b3a53a3bfb60ed0c1b`.
The frozen current source digest is
`87526e9dd70486aef5d43927f9b7106c69c50b6c016073aae998f6e1f8c7cce4`. Prepare
both noninstrumented release configurations before collecting any trial. Clear
inherited Rust/LLVM coverage configuration. Run no builds, tests, or coverage
concurrently with collection. Record each launch interval, actual order,
source identity before and after, platform/toolchain, load averages, and
available thermal observations. Unavailable host observations stay explicit.

No discarded slow samples, replacement trials, pooled favorable subsets, or
retries until green are allowed. A failed comparison remains failed and needs
investigation before any later, separately planned measurement. The original
failed and controlled cohorts in `program-status-performance.md` remain part
of the record. This scoped check does not establish full ADR qualification,
peak memory, tail latency, Linux performance, or coverage readiness.


## Preparation correction before any timed trial

The initial preparation in `work/coverage-closure/performance-controlled`
failed while building the baseline because that worktree has no local native
cache. No timed trial was launched (`execution.json` was never created).
The failure and log remain intact. Both configurations will explicitly use the
same verified native source/archive/library from the current worktree's cache,
with its path and SHA-256 hashes recorded and each configuration's verifier run
before building. This is a preparation repair, not replacement of a slow or
failed measurement. The one fixed cohort will be collected in
`work/coverage-closure/performance-controlled-final` under the unchanged plan.

## Independent final-cohort review — 2026-10-11

The reviewer independently checked every metadata file and all twenty records
at `work/coverage-closure/performance-controlled-final/pair-{0..4}-{baseline,current}`.
Actual order matches the prospective plan. All ten launch intervals are
nonoverlapping, each trial's metadata timestamp falls within its recorded
interval, and all launch exits are zero. Source digests are stable before/after
every trial and match the declared configuration; independent recomputation of
the current tree matches `87526e9d…f8c7cce4`.

Both configurations report macOS 26.6 arm64 and Rust 1.98.1 (`48a229cea`,
2026-09-01), the same release command/features, five trials, and 4 MiB/session.
The preparation manifest records the common native archive SHA-256
`820d84e8cbc4be0ca26b9a1b71cfdc5befdc814c414bef7b0dbcf51069668c68` and common
native library SHA-256
`1679af9dcd92e55d154a1206c40d4bef7c0f148d222157e7a1a84d1bcec83ecd`.
No initial preparation trial was substituted into this cohort.

Elapsed milliseconds and within-pair current/baseline ratios:

| Pair | 1-session baseline | 1-session current | Ratio | 16-session baseline | 16-session current | Ratio |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 45.458875 | 56.735208 | 1.248056 | 627.293917 | 549.223084 | 0.875543 |
| 1 | 59.185833 | 86.714375 | 1.465120 | 513.510875 | 1095.075583 | 2.132527 |
| 2 | 59.384625 | 51.101500 | 0.860517 | 476.911750 | 816.817625 | 1.712723 |
| 3 | 51.620375 | 60.238666 | 1.166955 | 606.872166 | 865.004250 | 1.425348 |
| 4 | 50.623916 | 64.552000 | 1.275129 | 881.968625 | 715.582584 | 0.811347 |

| Sessions | Baseline median ms | Current median ms | Change | Maximum accepted median ms | Result |
| --- | ---: | ---: | ---: | ---: | --- |
| 1 | 51.620375 | 60.238666 | +16.70% | 56.782413 | **Fail** |
| 16 | 606.872166 | 816.817625 | +34.59% | 667.559383 | **Fail** |

All records independently pass exact matching raw/parser byte totals of
4,194,326 and 67,109,222, the expected payload size, release build, no timeout,
successful completion, zero observed descendants, and five test-process file
descriptors after shutdown. These functional/cleanup results do not discharge
the elapsed-time failure. No slow samples were excluded.

One-minute load averages span 6.41–10.92. The thermal command reports no recorded
warning or power status, not a measurement of temperature or throttling. Within-
pair sixteen-session ratios range from 0.811 to 2.133; four of five one-session
pairs and three of five sixteen-session pairs are slower in the current build.
The different signs and magnitudes across cohorts prevent attributing the result
to code, scheduling, temperature or measurement conditions without further
evidence. They do not justify waiving either failure.

## Initial configuration inspection and next diagnostic

Read-only inspection found matching release test fingerprint feature sets
(`default`, `ghostty`), profile/configuration identifiers and empty `rustflags`
for both configurations. Their executable hashes differ, as expected for the
feature implementation: current
`b49bffb1382c4f4bcc611ac538adb1c72bcc6c59a129c251266444a8427aa6c0`
(3,552,304 bytes), baseline
`0ae27124c54ecc384ffef2b207541719861e637ef158fe25ae8aa526d2513b1c`
(3,511,856 bytes). These hashes were inspected after collection; the earlier
passing cohort did not preserve executable hashes, so binary identity across
those two dates cannot be established from this check alone.

The reviewed current changes since `5c7434a` in runtime/guardian module wiring
are test-only `cfg(test)` additions and a guardian test target. This supports
investigating build configuration and environment before changing the hot path;
it does not establish equivalence of every emitted instruction. The common
native library path controls that dependency within this cohort, while prior
cohort native binary identity remains unproved by its source hash alone.

Next, collect one explicitly diagnostic profile per configuration with the same
workload and noninstrumented binaries: record process-tree CPU time versus wall
time, thread activity/stacks, scheduling or blocking, and native/Rust hot spots.
Capture executable/native/helper identities and build flags before profiling.
Use those observations to distinguish extra parser/application work from waiting
or host scheduling. Profiling overhead makes these captures unsuitable as new
acceptance timing trials. Do not begin another five-pair qualification cohort
until the failed result has a documented, evidence-backed investigation and any
necessary fix has passed affected tests and independent review. The existing
failed cohort and earlier history remain intact.

## Intrusive profile review

The parent collected one `/usr/bin/sample` diagnostic per unchanged release
executable at `work/coverage-closure/profiles`. Metadata hashes match the inspected
failed-cohort binaries; both tests and both samplers exited successfully. These
captures span startup and both session-count phases and are explicitly excluded
from acceptance timing.

Both collapsed stack summaries contain many poll, semaphore, condition-variable
and mutex waits. The current scanner appears in 38 collapsed top-of-stack samples;
native `printSliceFill` appears in 117 current samples and 137 baseline samples.
These are wall-time observations across multiple threads, not normalized CPU
percentages. Different phase durations and idle threads make aggregate counts
unsuitable for asserting a regression's cause or measuring its size. In
particular, lower current mutex-wait counts do not prove less contention, and
waiting stacks do not prove external host interference.

The captures identify paths worth investigating but do not isolate a production
bottleneck, establish noise as the cause, or clear the failed guardrail. A useful
next diagnostic should isolate the output-processing interval and distinguish
on-CPU work from scheduling/blocking, or measure a specifically supported hot-path
hypothesis. No new acceptance cohort or production optimization is justified by
these broad stack counts alone.

## Isolated scanner experiment and scoped code-quality review

The subsequent isolated experiment at
`work/coverage-closure/scanner-experiment` tested a specific hypothesis: the
scanner's Ground-state scalar search spends avoidable CPU time on ordinary
chunks with no escape byte. Two copied dependency-free domain crates were built
in release mode before measurement. The only candidate production difference was
the three-line `if self.plain_chunk(input) { return; }` at scan entry. Five
alternating process pairs ran the existing domain example, which emits five
trials for each of its five workloads. This was diagnostic, not a substitute for
the failed integrated gate.

Both configurations used macOS 26.6 arm64 and Rust 1.98.1. Copied-domain digests
were `9f5196bc81b33c7cde865657fa00f39a63f6a09723b9b5f36c8e28b0210e16bf`
(original) and
`cb15214c12a69d0db4d4db257af915d78b86f6a66b60d65c440ba703f064e510`
(candidate), stable across collection. Exact commands, executable identities,
source copies, build logs and raw results remain in that directory.

Ordinary-text process medians for 8 MiB were 3.053541, 2.003125, 2.016125,
2.031375 and 2.017334 ms before; 0.586333, 0.286708, 0.271625, 0.258875 and
0.230667 ms with the candidate. Every pair improved (candidate/original
0.114–0.192). Other workload pair ratios ranged 0.899–1.127 for oversized
strings, 0.904–1.028 for queries, 0.990–1.014 for saturated records and
0.977–1.061 for single-byte chunks. Their small differences do not establish a
consistent improvement or regression. All non-timing example fields match
between corresponding trials, including record/query counts and logical heap
accounting. The candidate copy's seventy domain tests passed.

Independent review of the actual tracked three-line change finds no blocking
Clean Code, SOLID, dependency-direction or domain-invariant issue:

- The scanner owns its continuation state and makes the decision locally using
  its existing predicate. No policy moves into an infrastructure adapter, no
  dependency is added, and no abstraction, duplicate state or configurable switch
  is introduced.
- In Ground state with no escape byte, the former loop only searched for an
  escape and broke without modifying state or output vectors. Returning early
  has the same effect, including empty input and any retained inactive buffer.
- Escape, OSC and generic-string continuation states fail the predicate even
  when the next chunk has no escape byte. Their payload, cancellation bytes,
  terminators and offsets therefore follow the unchanged parser. Chunks with
  an escape byte likewise keep the original path.
- The shortcut is inside scanner execution; public store validation, plan
  identity, overflow checks, transactional commit and generation advancement are
  unchanged. It does not bypass stale-plan protection or admission checks.
- The code remains bounded and linear. Escape-bearing Ground chunks may incur
  an extra scan, so the optimization is specifically justified by the measured
  ordinary-output case, not by an assertion that every input becomes faster.

This code-review conclusion rests on the control-flow/state equivalence above,
not on benchmark success. Independent adversarial continuation tests, affected
mechanical checks and final-source coverage remain separate acceptance duties.
The reproducible domain benefit justifies this narrow candidate change, but its
roughly 1.75 ms saved per 8 MiB does not explain or discharge the previous
integrated failures.

## Prospective integrated acceptance for the scanner candidate

This materially changed candidate gets one new, fixed controlled cohort after
all affected correctness reviews, mechanical checks and coverage work finish.
Freeze and record its full source digest **before** collecting measurements;
the earlier `87526e9d…f8c7cce4` digest does not identify the changed scanner.
Record the new digest in the preparation manifest and final review. Baseline
remains `2fca6559` with digest
`c93dbb7ba13f01a3dae26a42070b503e8129cdccdacac9b3a53a3bfb60ed0c1b`.

Use the same five pairs in fixed B,C / C,B / B,C / C,B / B,C order, unchanged
release harness, 4 MiB per session, one and sixteen sessions, same host/toolchain
and verified common native dependency. Prepare clean noninstrumented binaries
before measurements and record binary/native/helper hashes and build settings.
No coverage, profiling, build or test process may overlap collection. Preserve
all launch intervals, environment observations, raw results and source hashes.

At both counts, the candidate cohort median divided by the contemporaneous
baseline median must be at most 1.10. Every trial must satisfy exact byte totals,
successful completion, no timeout and zero observed descendants; inspect those
fields independently. Publish all samples, cohort ranges and within-pair ratios;
paired ratios remain diagnostic. No dropped samples, replacement trials, pooling
with previous cohorts or retrying until green. A failed or incomplete planned
cohort remains failed/incomplete and requires investigation before another plan.
The prior failed cohorts remain part of the record whatever this candidate's
result; no speedup, full ADR readiness or broader coverage claim follows from a
scoped pass.

## Final scanner-candidate cohort: independent review

**Verdict: FAIL at sixteen sessions; PASS at one session.** No new cohort was
started by the reviewer. All five planned pairs and all twenty records from
`work/coverage-closure/performance-scanner` were independently inspected. Actual
order matches B,C / C,B / B,C / C,B / B,C; recorded intervals do not overlap,
and trial metadata timestamps fall inside their corresponding launch interval.
Every launch/test exit is zero. The driver's final failure reflects the timing
threshold rather than a crashed or missing trial. No samples were omitted.

The frozen current digest is
`07ea3217667be0e547ec5fd403ef8175c234dc98832a3cca6facb3158c29c092`, independently
recomputed after collection. Every current trial reports that same stable
before/after identity. Baseline remains
`c93dbb7ba13f01a3dae26a42070b503e8129cdccdacac9b3a53a3bfb60ed0c1b`, likewise
stable across every trial. Both configurations used macOS 26.6 arm64,
Rust 1.98.1 (`48a229cea`, 2026-09-01), the unchanged native-projected release
command, and 4 MiB per session. The manifest reports instrumentation environment
absent. The prepared native archive/library hashes are identical to the preceding
cohort and were independently rehashed successfully.

Premeasurement `build-artifacts.json` and postmeasurement independent rehashes
agree on these executable identities:

| Configuration | Artifact | SHA-256 | Bytes |
| --- | --- | --- | ---: |
| Current | Runtime performance executable | `b096297af0160e46c0e17ffe19dbafc27074667b0a82bd068ddc7e26551ccf41` | 3,552,304 |
| Current | Guardian | `cbd3d46ec786bad998458660f248e658e13e1df46a38354f8505fbfbeda43901` | 338,368 |
| Baseline | Runtime performance executable | `0ae27124c54ecc384ffef2b207541719861e637ef158fe25ae8aa526d2513b1c` | 3,511,856 |
| Baseline | Guardian | `a9e9e852bd402cae45fcd08c03daf04dd8c20701dffa29c8b687284783b0195e` | 338,368 |

Different guardian hashes are recorded facts, not evidence of a behavior change;
equal sizes are also insufficient to prove binary equivalence.

Elapsed milliseconds and diagnostic within-pair current/baseline ratios:

| Pair | 1-session baseline | 1-session current | Ratio | 16-session baseline | 16-session current | Ratio |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 46.764916 | 52.034000 | 1.112672 | 344.144500 | 584.350708 | 1.697981 |
| 1 | 52.716166 | 48.890958 | 0.927438 | 350.558875 | 434.551291 | 1.239596 |
| 2 | 47.724417 | 48.492667 | 1.016098 | 454.291084 | 361.540000 | 0.795833 |
| 3 | 48.390750 | 47.613000 | 0.983928 | 584.064417 | 362.018292 | 0.619826 |
| 4 | 49.897000 | 47.976000 | 0.961501 | 360.339000 | 595.164042 | 1.651678 |

| Sessions | Baseline median ms | Current median ms | Change | Maximum accepted median ms | Result |
| --- | ---: | ---: | ---: | ---: | --- |
| 1 | 48.390750 | 48.492667 | +0.21% | 53.229825 | Pass |
| 16 | 360.339000 | 434.551291 | +20.60% | 396.372900 | **Fail** |

Baseline/current one-session ranges are 46.76–52.72 / 47.61–52.03 ms;
sixteen-session ranges are 344.14–584.06 / 361.54–595.16 ms. Sixteen-session
paired ratios range 0.620–1.698, with three of five pairs slower in the current
build. Recorded one-minute load averages range 9.74–12.72. Thermal output again
reports no recorded warning/power status rather than measured temperature or
throttling. These observations limit causal attribution but do not change the
predeclared failed result.

All twenty records independently satisfy exact raw/parser totals (4,194,326 and
67,109,222 bytes), expected payload size, release build, no timeout, successful
completion, zero observed descendants and five remaining test-process file
descriptors. Source stability and cleanup passed; elapsed-time acceptance did
not. The isolated scanner cost reduction therefore has not produced a qualified
integrated result under the declared method.

Handoff: retain this failed cohort, the previous failures and the earlier pass.
Do not promote the PR as ready on the strength of mechanical success, domain
microbenchmarks or a selected earlier timing cohort. Further performance work
requires an evidence-backed investigation and a separately declared plan;
coverage readiness remains an independent blocker tracked in its own ledger.
