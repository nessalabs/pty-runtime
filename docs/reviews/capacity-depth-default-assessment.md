# Capacity-depth default assessment

## Recommendation

**Retain 256 as the release default for now; use 32 as the strongest next candidate and an opt-in setting for this saturation profile.** The complete Mac experiment supports a large queue-latency and memory benefit at 32. It does not establish a repeatable depth-caused throughput regression, but also does not qualify a global default change: trial blocks were chronological rather than interleaved, the unchanged control drifted materially, and this is one host's unpaced saturation workload—not the controlled 10 MiB/s reference or Linux qualification.

This is a decision to retain the existing baseline while evaluating the candidate, not an assertion that 256 passes the saturation latency target or that release qualification is complete. All ten retained 256 trials fail that target. All five 32 trials pass. No product change, native investigation, build, or workload was performed for this assessment.

## Evidence and calculation

As of September 8, 2026, the complete sources are [20 depth-sweep trials](../verification/release/candidate5-depth-sweep/summary.json) and [five late 256-slot controls](../verification/release/candidate5-depth-post-control/summary.json). All 25 record the same frozen source `71c1d6f42bbd1c547f747fdd7d20ed69e3000c4e`, empty diff, source inventory, binary SHA-256 `e432d7f00dd0329be8f1d3b29e303e7aa5a0a5730b38fa2c511e879342ccc6c3`, and host identity: Apple M5, 10 logical CPUs, 24 GiB RAM, macOS 26.6 arm64. The identity record cautions that hashes alone do not prove the source-to-binary build link; retain the coordinating agent's build evidence with these trials.

Every case uses 64 projected sessions, 16 active producers, unpaced output, 4093-byte chunks, one observer per session, 80×24 grids, 10-second warmup and 60-second measurement. Only per-session staging slots vary. Analysis uses phase 1 producer records; warmup is excluded. Five trial summaries per group are compared with equal-trial medians and ranges. Percent changes are ratios of these medians, not a claim about pooled-event p99. Individual producer data remain available separately.

Run the retained [calculation script](../verification/release/candidate5-depth-assessment/analyze.py) from any directory. It reads existing JSONL only and produces [analysis.json](../verification/release/candidate5-depth-assessment/analysis.json), [25 trial rows](../verification/release/candidate5-depth-assessment/trials.csv), [400 active producer rows](../verification/release/candidate5-depth-assessment/producers.csv), and an [input SHA-256 manifest](../verification/release/candidate5-depth-assessment/input-sha256.txt). It asserts complete trial/summary agreement, identical identities, all 64 producer identities per phase, exact byte ledgers, throughput numerator/denominator consistency, finite caps, complete latency samples, zero post-forget budgets, and no recorded zombies. The original input files are unchanged.

| Artifact | SHA-256 |
| --- | --- |
| Calculation script | `f4af9dba207aa98aaf1f76013dd19845cfe694c1a194152b53ffee23d64d36e1` |
| Analysis JSON | `08d57f262803c9ed4ad117f61c0e95449fcab3a21858c9390acba298112a2549` |
| Input manifest (all 25 trials and both summaries) | `cca69f884098233664333c5e386ed12b2cbab7f3ca2ddb34f72e054a6bda981c` |

## Latency and throughput

Each cell is the median of five trials, with min–max where shown. MB/s means decimal megabytes per second; memory below uses binary MiB.

| Chronological group | Throughput MB/s | Projected-output p99 ms | Resize p99 ms | Latency-target passes |
| --- | ---: | ---: | ---: | ---: |
| 256 early | 57.30 (55.27–63.09) | 89.5 (78.2–90.7) | 89.4 | 0/5 |
| 128 | 55.37 (54.89–56.25) | 46.9 (46.5–47.2) | 47.0 | 0/5 |
| 64 | 54.53 (52.13–54.58) | 24.5 (24.2–24.9) | 24.6 | 0/5 |
| 32 | 51.53 (51.24–52.14) | 13.4 (13.3–13.7) | 13.3 | 5/5 |
| 256 late control | 51.89 (50.92–55.14) | 95.3 (89.7–96.8) | 95.2 | 0/5 |

The 32-slot projected-output p99 is 85.03% below the early control and 85.94% below the late control. All five 32 trials have margin below 20 ms; 64 does not meet that target. These are host-read-completion-to-projection measurements and do not include time a producer spends waiting before the host read.

Throughput in chronological trial order is:

- Early 256: 63.09, 59.27, 57.30, 56.42, 55.27 MB/s.
- 128: 56.25, 55.81, 55.37, 54.89, 55.21 MB/s.
- 64: 54.58, 54.57, 54.53, 53.04, 52.13 MB/s.
- 32: 52.14, 51.53, 52.02, 51.37, 51.24 MB/s.
- Late 256: 55.14, 50.92, 52.37, 51.89, 51.14 MB/s.

The 32 median is **10.07% below early 256** and **0.69% below late 256**. Means tell the same cautionary story: −11.34% and −1.21%, respectively. Unchanged 256 itself drifts −9.44% by median and −10.26% by mean. The control cannot identify why—thermal state, scheduling or other host conditions were not isolated—but it prevents assigning the whole early-to-32 decline to staging depth. Do not choose only the early or late control to manufacture a regression/pass claim. These trials do not estimate an unconfounded causal throughput effect.

## Producer cost and backpressure

Producer writes use a nonblocking descriptor. `write_syscall_ns` measures time inside write calls; `write_blocked_ns` separately measures POLLOUT waits. Raw counts depend on how many bytes completed, so the comparison includes counts/time per accepted GiB. The following are medians of trial-level totals across 16 active producers:

| Metric | 256 early | 32 | 256 late |
| --- | ---: | ---: | ---: |
| Sum of write-syscall time, seconds | 26.022 | 27.240 | 27.005 |
| Write-syscall seconds per GiB | 8.030 | 9.460 | 9.316 |
| EAGAIN count | 3,345,928 | 3,007,408 | 3,028,764 |
| Partial-write count | 3,349,957 | 3,011,634 | 3,032,950 |
| EAGAIN per GiB | 1,045,440 | 1,044,046 | 1,044,011 |
| Partial writes per GiB | 1,046,467 | 1,045,659 | 1,045,648 |

At 32, syscall seconds/GiB rise 17.81% against early 256 but only 1.54% against late 256; unchanged controls rise 16.02%. EAGAIN and partial-write rates per GiB are effectively unchanged against either control. Lower absolute counts at 32 mostly accompany fewer bytes rather than proving less pressure.

To avoid hiding individual producers, the next table summarizes all 80 active producer observations per group (16×5). These observations are clustered within trials; they are not 80 independent experiments.

| Per-producer statistic, median (range) | 256 early | 32 | 256 late |
| --- | ---: | ---: | ---: |
| Throughput MB/s | 3.593 (3.405–4.105) | 3.217 (3.142–3.385) | 3.237 (3.127–3.572) |
| Write-syscall seconds | 1.625 (1.358–1.778) | 1.702 (1.595–1.894) | 1.682 (1.600–1.812) |
| Maximum write-call duration, ms | 0.499 (0.292–11.877) | 0.629 (0.287–6.431) | 0.608 (0.373–7.916) |
| POLLOUT-wait seconds | 58.025 (57.869–58.305) | 57.944 (57.733–58.052) | 57.962 (57.823–58.055) |
| Maximum POLLOUT wait, ms | 4.542 (2.630–35.024) | 8.068 (3.458–52.985) | 6.233 (3.580–35.383) |

The unpaced producers spend most of each minute backpressured at every depth. Total wait time is nearly unchanged at 32 versus late 256, and no active producer is starved of all output. However, the median of per-producer maximum waits rises 6.23→8.07 ms (+29.4%) against late 256, and the largest observed wait is 52.99 versus 35.38 ms. These are maxima, **not write-wait p99**, and cannot prove or dismiss a p99 regression. The retained telemetry has no full write-wait distribution. Smaller admitted queues can move delay before the measured host-read boundary; the stage-latency result must travel with this limitation.

## Fixture RTT and CPU/memory

Fixture RTT p99 medians are 4.6 ms early 256, 19.1 ms at 128, 22.1 ms at 64, 14.6 ms at 32 and 89.5 ms late 256. The late-control range is 4.7–90.6 ms; the 32 range is 14.5–14.9 ms. There are no recorded RTT failures or unavailable observations, but the measurement is not an independent child-read timestamp. `phase.rs::controls` records elapsed time when the harness consumes an acknowledgement. The phase loop awaits an ordered resize before it next services controls, so resize and harness service delay can enter RTT. Consequently, neither the early +217% nor late −84% comparison isolates user-perceived input responsiveness. Runtime input-dispatch p99 is 0.8 ms early and 1.1 ms both at 32 and late 256; the same chronology caveat applies to the relative +37.5% against early control.

| Median metric | 256 early | 128 | 64 | 32 | 256 late |
| --- | ---: | ---: | ---: | ---: | ---: |
| Owner CPU, % of one core | 682.27 | 692.20 | 689.86 | 678.54 | 680.13 |
| Workload-fixture CPU, % of one core | 95.62 | 96.96 | 100.01 | 99.66 | 99.30 |
| Sampled owner peak RSS, MiB | 52.70 | 51.53 | 48.05 | 48.48 | 51.61 |
| Rust requested allocation peak, MiB | 24.03 | 20.97 | 19.44 | 18.67 | 24.03 |
| Sampled staged bytes peak, MiB | 4.00 | 2.00 | 1.00 | 0.50 | 4.00 |

Owner CPU at 32 is within 0.6% of either control's median. CPU per throughput unit is +9.96% against early and −0.74% against late control, again consistent with material chronology effects. CPU includes census overhead and only matched snapshot PIDs; short-lived processes can be unmeasured. Darwin PSS and portable wakeup counts are unavailable. RSS is a sampled peak, not continuous allocation accounting, and does not include the whole tree here.

The requested allocation peak falls about 22.3%, with much less sampled staged data. These requested bytes include fixture bookkeeping and are not a native/RSS or 4 KiB control-memory proof. The reader scratch allocation remains exactly 262,144 bytes across 64 readers in every trial. Changing staging depth does not reduce that allocation.

## Correctness, release rule and limits

All 25 trials complete their full duration and pass execution/correctness accounting. The script checks every producer's final verified+gap ledger against total produced bytes; all gaps are zero here, no byte cap is exhausted, resource budgets stay within bounds and return to zero after forgetting, and no sampled zombies appear. These checks do not replace canonical terminal reference tests or the long soak.

[ADR 0002](../adr/0002-performance-and-stability.md) requires a reviewed host-specific baseline and treats a **repeatable regression greater than 10%** in throughput or p99 latency as a review failure pending explanation and acceptance. The early throughput comparison crosses that threshold and remains explicit. The late control supplies a substantial alternative explanation, so this is not proof of a repeatable 32-depth regression. Equally, the late control is not permission to certify all latency/throughput metrics unaffected. Histogram resolution, fixture RTT servicing, missing writer-wait p99 and block ordering limit that claim.

The ADR's controlled 64-session/16-producer **10 MiB/s** reference remains distinct from this approximately 51–63 decimal MB/s unpaced capacity experiment. Passing the 20 ms line at one capacity point does not complete the controlled-profile, detached/dominant-producer, other-size, Linux, high-session-count or stability matrix. A global default change should follow a reviewed controlled comparison on supported hosts, ideally interleaved 256/32 trials with independent acknowledgement timestamps or explicit harness-delay accounting. No such additional run is authorized or executed by this review.

Validation status: **share with caveats**. The retained calculations and bounded Mac conclusion are inspectable and consistent; the broader release-default effect is not established. The best supported next configuration to qualify is 32, while 256 remains the current release baseline.
