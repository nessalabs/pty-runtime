# Program status performance acceptance

Current scoped verdict: the prospectively planned controlled comparison below
**passes** at one and sixteen sessions. The earlier final-source batch failed
and remains recorded; neither result establishes a causal speedup or regression.

Scoped guardrail declared before collecting integrated stage 2 results:
median elapsed time must not regress by more than 10% at either 1 or 16
sessions against the same-host baseline; all trials must validate every byte,
complete without timeout, and leave zero descendant processes after shutdown.
Use five release-build trials per configuration. This is an ordinary-output
regression check, not a full ADR 0004 qualification or a tail-latency claim.

Baseline: `2fca6559e95244f8b935ba30a71ef7241d23da09`, five trials of
`scripts/performance.py`, 4 MiB per session, artifacts in
`../pty-runtime-program-status-baseline/work/osc7501-baseline/trial-{0..4}`.
Each metadata file records platform, toolchain, source hash, command and stability.
The baseline was collected before runtime integration. Stage 2 measurements are
recorded below. Record load and variance limits alongside any comparison; process RSS
point samples do not measure peak memory or child/helper memory.

Domain adversarial workloads are measured independently with five repeats by
`scripts/program_status_performance.py`: ordinary text, oversized control strings,
query floods, saturated records and single-byte chunks. Deterministic reservation
checks cover logical allocation capacity, excluding allocator metadata; timing
results alone do not prove memory admission or bounded retention.

## Independent scoped evidence review — 2026-10-11

The following initial integrated result is preserved as historical evidence.
The exact final-source repetition below supersedes its acceptance verdict and
**fails the sixteen-session guardrail**.

**Verdict: the declared ordinary-output median guardrail passes at both session
counts.** This is a scoped measurement result, not a release qualification or a
causal speedup claim. The reviewer read the harness and independently checked all
ten baseline and ten integrated records, metadata, exact expected byte counts,
and the current source digest. No benchmarks were run by the reviewer alongside
the measurements.

Both configurations used five release-build trials with 4,194,304 payload bytes
per session on macOS 26.6 arm64, Rust 1.98.1 (`48a229cea`, 2026-09-01), with
`cargo test --locked --release --test runtime_performance --features ghostty --
--ignored --nocapture --test-threads=1`. The operator reports the same physical
host and no concurrent test/build workload during the integrated trials;
background system load was not recorded. Each trial measured from release of the
producers through raw validation and native parser completion; startup and
shutdown are recorded separately. The producer, Rust performance test, and Python
runner are byte-identical between the baseline and integrated worktrees.

| Configuration | Source identity | Raw evidence |
| --- | --- | --- |
| Before | Commit `2fca6559e95244f8b935ba30a71ef7241d23da09`; digest `c93dbb7ba13f01a3dae26a42070b503e8129cdccdacac9b3a53a3bfb60ed0c1b` | `../pty-runtime-program-status-baseline/work/osc7501-baseline/trial-{0..4}` |
| Integrated | Base commit `3490f98ac9282ebe36a496121cbf40adc6474eba` plus uncommitted integration; digest `b5284b7163a2d482efb33d27394327874056c58cffb91ae6596b08d1f47744f8` | `work/osc7501-stage2/performance/trial-{0..4}` |

All before/after digests were stable within every trial and identical across the
five trials of each configuration. The integrated digest matches the reviewed
source tree. Commit identity alone does not identify that uncommitted code.

Elapsed times below are milliseconds; deltas compare medians.

| Sessions | Before trials 0–4 | Integrated trials 0–4 | Before median | Integrated median | Delta | Maximum accepted median |
| --- | --- | --- | ---: | ---: | ---: | ---: |
| 1 | 46.831208, 46.915500, 41.019625, 49.305750, 46.463542 | 43.882875, 46.604125, 46.316583, 46.245625, 45.989375 | 46.831208 | 46.245625 | −1.25% | 51.514329 |
| 16 | 358.288500, 511.109375, 548.123458, 365.079375, 525.981208 | 343.824250, 476.731834, 582.207167, 573.635750, 497.006500 | 511.109375 | 497.006500 | −2.76% | 562.220313 |

Every trial exited successfully without timeout and validated the exact raw and
parser totals: 4,194,326 bytes for one session and 67,109,222 for sixteen sessions,
including READY and terminal suffix bytes. The test also checks the final native
terminal text, producer exit code zero, EOF drain and closed projection residency.
Every row reports zero observed descendants after shutdown and five remaining
test-process file descriptors.

### Findings and limitations

- The cleanup guard is not automatically enforced by `scripts/performance.py`:
  the Rust test records descendant counts and the runner checks other fields.
  Independent inspection of all twenty rows resolves the cleanup-evidence
  question for this dataset. Future acceptance must repeat that check or add an
  explicit validator; a successful runner exit alone is insufficient.
- The process sampler sees the current descendant tree only. It cannot establish
  that no already-reparented process escaped. The zero counts support observed
  descendant cleanup for these cooperative producers, not universal leak freedom.
- Sixteen-session elapsed ranges are wide: 358.29–548.12 ms before and
  343.82–582.21 ms integrated. Runs were collected in configuration blocks, not
  randomized pairs. Unmeasured load and scheduling variance prevent attributing
  the small median difference to the code. The predeclared median target passes;
  no tail-latency target or statistical speedup was proved.
- RSS fields are point samples of the test process, excluding producer/helper
  processes. Neither peak memory, allocation admission, nor cleanup of every
  resource follows from those samples. This workload covers ordinary output and
  does not measure integrated OSC query/report floods, active-record scaling, or
  other operating systems.

No blocking performance finding remains for this narrow ordinary-output
comparison. Broader runtime readiness still depends on the mechanical gate,
independent design/correctness reviews, coverage, and full ADR workloads.

### Domain workload evidence

The independent domain-only dataset at
`work/program-status-performance/20261011T163959Z-cfbb5029` has the same integrated
source digest, platform and toolchain above, stable source metadata, successful
exit, and five release trials for each workload. Median elapsed times are:

| Workload | Bytes per trial | Median ms |
| --- | ---: | ---: |
| Ordinary text | 8,388,608 | 3.807291 |
| Oversized control strings | 8,415,232 | 11.435875 |
| Query flood | 4,718,592 | 5.489125 |
| Saturated records | 650,880 | 13.264209 |
| Single-byte chunks | 184,320 | 10.052625 |

These are observations without a pre-change timing comparison or speed target.
The harness asserts expected record/query counts and logical retained-capacity
bounds, with allocation preflight outside the timer. It excludes the PTY,
application admission/queues, native parser, allocator metadata and OS memory;
it therefore cannot qualify integrated flood responsiveness or total memory.

## Final-source repetition and independent re-review — 2026-10-11

**Verdict: FAIL at sixteen sessions; the scoped performance gate is blocked.**
The one-session result passes. This replaces the initial integrated acceptance
verdict above; the earlier dataset remains evidence and is not discarded.

After final test additions, five fresh release trials were collected at
`work/osc7501-stage2/performance-final/trial-{0..4}`. The recorded base commit is
still `3490f98ac9282ebe36a496121cbf40adc6474eba`, with uncommitted source digest
`aaf8eabeb3597197fedffaebba148c631566499d77df1ca81de634df0b62bf8f`. All five trials
have matching before/after digests, and an independent recomputation matches the
current source tree. The operator reports production and harness unchanged from
the initial integrated dataset; the digest changed after test additions.

The final and baseline metadata agree on macOS 26.6 arm64, Rust 1.98.1
(`48a229cea`, 2026-09-01), and the exact release command recorded above. Workload
remains 4,194,304 payload bytes per session, five trials at each session count.
The baseline remains the original `trial-{0..4}` dataset at commit `2fca6559`;
neither the baseline nor the predeclared 10% median threshold was changed.

Elapsed times are milliseconds:

| Sessions | Final trials 0–4 | Baseline median | Final median | Delta | Maximum accepted median | Result |
| --- | --- | ---: | ---: | ---: | ---: | --- |
| 1 | 46.948166, 46.999042, 47.676791, 48.551500, 49.997541 | 46.831208 | 47.676791 | +1.81% | 51.514329 | Pass |
| 16 | 569.580709, 374.365958, 623.483208, 637.919084, 682.820375 | 511.109375 | 623.483208 | +21.99% | 562.220313 | **Fail** |

Independent validation of all ten final records confirms release builds,
successful exit without timeout, exact raw/parser totals of 4,194,326 and
67,109,222 bytes, zero observed descendants after shutdown, and five remaining
test-process file descriptors. Functional byte validation and observed cleanup
therefore pass; they do not override the elapsed-time failure.

Final sixteen-session timings span 374.37–682.82 ms. Background load and thermal
state were not measured, and configuration blocks were not randomized. These
limits prevent identifying the cause of the failed comparison: the data do not
prove a code-caused regression, but they also do not meet the declared guardrail.
Selecting the earlier passing dataset or attributing this result to noise without
further evidence would not resolve the failure. Keep qualification blocked until
a documented investigation and an independently reviewed comparison resolve it.
The original domain-only observations above remain tied to their earlier digest;
they are not represented as a fresh final-source domain run.

## Prospective controlled investigation — declared before collection

The failed final-source batch remains failed. One additional, fixed comparison
will investigate temporal confounding by collecting a contemporaneous baseline
beside each integrated trial. This changes collection order, not the workload,
source implementations, elapsed-time statistic or 10% threshold. It does not
retroactively turn the earlier failed cohort into a pass.

Collect exactly five pairs, with both configurations on the same physical host
using the unchanged release harness and 4 MiB/session workload. Preserve every
trial, including failures. The order is fixed in advance:

| Pair | First | Second |
| --- | --- | --- |
| 0 | Baseline | Integrated |
| 1 | Integrated | Baseline |
| 2 | Baseline | Integrated |
| 3 | Integrated | Baseline |
| 4 | Baseline | Integrated |

Use baseline source digest
`c93dbb7ba13f01a3dae26a42070b503e8129cdccdacac9b3a53a3bfb60ed0c1b` and integrated
digest `aaf8eabeb3597197fedffaebba148c631566499d77df1ca81de634df0b62bf8f` with the
same toolchain, features and command as the preceding datasets. Verify identity
before and after every trial. Both release configurations are already built;
do not overlap measurement with coverage, builds, tests or another benchmark.
The operator will wait for the current coverage run to finish before starting.
Record execution order and timestamps. Where available, collect lightweight
load-average and power/thermal observations outside the timed workload; explicitly
record unavailable environmental measurements instead of assuming an idle host.

The decision remains the ratio of the integrated cohort's median elapsed time to
the contemporaneous baseline cohort's median: it must be at most 1.10 at **both**
one and sixteen sessions. Every trial must satisfy the existing exact-byte,
completion and observed-descendant-cleanup conditions. Independently inspect all
records, because the cleanup requirement is not enforced by the runner. Also
publish each within-pair integrated/baseline ratio, cohort ranges, all raw values
and source/platform metadata. Pair ratios are diagnostics, not a replacement
acceptance statistic selected after seeing results.

Do not exclude slow samples, stop early when results look favorable, pool selected
trials from different cohorts, or retry until green. A timeout, source change or
other invalid trial stays in the record and prevents treating this planned set as
a complete successful qualification. If this controlled comparison fails the
guardrail, the next step is profiling and a justified implementation change,
followed by a newly documented verification plan. If it passes, report that scoped
result alongside the failed earlier cohort; it still does not establish a causal
speedup, peak-memory safety, tail latency or full ADR qualification. Five pairs
reduce temporal separation but do not eliminate scheduling or thermal variance.

## Controlled comparison results and independent re-review — 2026-10-11

**Verdict: PASS for the predeclared controlled ordinary-output guardrail at both
session counts.** The preceding failed batch remains failed. This completed
investigation provides scoped contemporaneous evidence; it does not erase the
failure, waive its threshold, or establish that background load caused it.

Evidence: `work/osc7501-stage2/performance-controlled/pair-{0..4}-{baseline,current}`,
with `execution.json` recording actual order/environment and `prepare.log`
recording release builds before collection. Independent inspection confirms the
exact prospectively specified alternating order, ten successful nonoverlapping
launch intervals, metadata timestamps within those intervals, and all twenty
one/sixteen-session records. No trial was omitted or replaced.

Per-configuration identity is unchanged from the prospective plan:

- Baseline: commit `2fca6559e95244f8b935ba30a71ef7241d23da09`, source digest
  `c93dbb7ba13f01a3dae26a42070b503e8129cdccdacac9b3a53a3bfb60ed0c1b`.
- Integrated: base commit `3490f98ac9282ebe36a496121cbf40adc6474eba` plus
  uncommitted integration/tests, source digest
  `aaf8eabeb3597197fedffaebba148c631566499d77df1ca81de634df0b62bf8f`.

All before/after trial digests match their respective configuration, and the
integrated digest matches an independent current-tree recomputation. Both cohorts
used macOS 26.6 arm64 on the recorded Apple M5, Rust 1.98.1 (`48a229cea`,
2026-09-01), the unchanged release command above, 4 MiB/session, and five trials
per session count. The operator reports no concurrent build/test/coverage work.
Recorded one-minute load averages ranged from 7.57 to 9.13; they are coarse
system-wide observations, not CPU utilization or proof of idle conditions.
The thermal command reported no recorded warning/power status: actual temperature,
frequency and throttling measurements are unavailable.

Elapsed times below are milliseconds. Each ratio is integrated/baseline within
the same pair and is diagnostic only.

| Pair | 1-session baseline | 1-session integrated | Pair ratio | 16-session baseline | 16-session integrated | Pair ratio |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 49.380500 | 47.316042 | 0.958193 | 589.490958 | 474.445666 | 0.804840 |
| 1 | 47.419167 | 47.876000 | 1.009634 | 683.354333 | 354.185250 | 0.518304 |
| 2 | 47.079708 | 44.607333 | 0.947485 | 597.061125 | 570.906084 | 0.956194 |
| 3 | 48.266084 | 46.062834 | 0.954352 | 558.183292 | 363.870958 | 0.651884 |
| 4 | 46.766083 | 46.232750 | 0.988596 | 708.497250 | 486.915417 | 0.687251 |

| Sessions | Baseline median | Integrated median | Change | Maximum accepted median | Result |
| --- | ---: | ---: | ---: | ---: | --- |
| 1 | 47.419167 | 46.232750 | −2.50% | 52.161084 | Pass |
| 16 | 597.061125 | 474.445666 | −20.54% | 656.767238 | Pass |

Every record has a successful release-build result without timeout, exact matching
raw/parser byte totals (4,194,326 at one session, 67,109,222 at sixteen), zero
observed descendants and five test-process descriptors after shutdown. Cleanup
counts were manually validated rather than inferred from the runner's success.

One-session ranges are 46.77–49.38 ms baseline and 44.61–47.88 ms integrated;
sixteen-session ranges are 558.18–708.50 ms baseline and 354.19–570.91 ms
integrated. All five sixteen-session paired ratios are below one, including both
execution orders, but their 0.518–0.956 spread is large. The small fixed cohort,
coarse load data, unmeasured thermal state and variation across earlier cohorts
do not justify a sustained 20.54% speedup claim or prove the cause of the earlier
failure. No production optimization is justified solely by these measurements.

The controlled scope is now qualified against its unchanged median target; there
is no remaining blocking finding for this ordinary-output comparison. Overall
runtime acceptance remains separate: native/protocol correctness, other platforms,
coverage, adversarial integrated floods, peak memory, tail latency and full ADR
workloads are not established by this result.
