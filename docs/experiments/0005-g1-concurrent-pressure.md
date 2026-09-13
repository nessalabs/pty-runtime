# Experiment 0005: G1 concurrent output and pressure

**Gate:** G1 (process and bytes), [ADR 0004](../adr/0004-integration-and-release-qualification.md) ·
**Recording rules:** [ADR 0002](../adr/0002-performance-and-stability.md) ·
**Data:** [`data/0005/`](data/0005/)

## Why this exists

[`docs/verification.md`](../verification.md) recorded G1 as unproven for one
reason: its qualifier is "pass **concurrent pressure tests**", and the largest
concurrency ever executed against the runtime was 16 producers, against ADR
0004's 64-session controlled reference and 128-session primary scenarios. The
harness for those scenarios already existed in `scripts/release/load.py`; nobody
had run it and recorded the result.

This is that run, on two platforms.

## Verdict

**G1 is not signed off. Its correctness criteria pass on both platforms; its
latency criterion fails reproducibly, and the failures are not host noise.**

This verdict is for `e01c704`. A follow-up measurement of a candidate fix, at a
different revision, is recorded in [Follow-up: chunk
batching](#follow-up-chunk-batching) below; it does not change this verdict,
because it re-ran 4 of the 26 cases.

| | macOS arm64 | Linux x86_64 |
| --- | --- | --- |
| Trials completed | 115 of 130 | **130 of 130** |
| Correctness failures | **0** | **0** |
| Trials missing a latency target | 10 | **20** |
| Harness failures | 15 | 0 |

The second column is the evidence that matters. It was measured on a dedicated
machine doing nothing else, and it is *worse* than the noisy laptop — which is
what rules out contention as the explanation.

## What was run

26 cases × 5 trials, 60 s each after a 10 s warmup, per
[`scripts/release/LOAD.md`](../../scripts/release/LOAD.md): the 64-session /
16-active / 10 MiB/s controlled reference in attached, detached,
stalled-observer, stalled-event-stream-sink and dominant-producer modes;
unpaced `capacity-projected` and `capacity-raw`; **128 independent active
producers** and 128-session mixed populations; rate, chunk, observer and grid
sweeps; 64 raw idle sessions; and raw/projected resource measurement at 1, 32
and 128 sessions.

Both hosts ran the identical source: `source_head e01c704`, clean tree
(`diff_sha256` is the empty-input SHA-256), 311 files inventoried by hash.

| | macOS | Linux |
| --- | --- | --- |
| Platform | macOS 26.6, Darwin 25.6.0, arm64 | Ubuntu 24.04, Linux 6.8.0-117, x86_64 |
| Host | Apple M5, 10 logical CPUs, interactive desktop under load | 8 vCPU / 15 GiB dedicated cloud box, otherwise idle |
| rustc | 1.98.1 | 1.97.1 |

Both clear the 1.85 MSRV. The compiler differs between hosts and is recorded
rather than reconciled.

## Correctness: passes

No trial failed a correctness or accounting check on either host. 145.6 GiB
(macOS) and 151.7 GiB (Linux) of delivered bytes were verified against their
expected values at absolute offsets.

**Replay eviction is accounted, not silent.** Four cases report gap bytes —
`detached` and `stalled-observer`, where no one is reading and the 1 MiB
per-session retention cap engages by design, and the two unpaced `capacity`
cases, where producers outrun retention. The other **22 of 26 cases report
exactly zero gap bytes**, including `attached`, `128-active` and `128-mixed`.
ADR 0004's criterion is not that eviction never happens; it is that "any
observer replay loss has an exact cursor gap", which is what the ledger shows.

Paced cases held their offered rate: `attached`, `128-active`, `128-mixed` and
`chunk-64` all accepted **10.0 MiB/s** on both hosts. The latency misses below
are therefore not the runtime falling behind on throughput — it meets the byte
target while missing the tail-latency target.

## Latency: fails, reproducibly

Targets come from ADR 0002 and are checked per boundary. Misses, with observed
p99 against target:

| Case | Boundary | Target | macOS | Linux |
| --- | --- | ---: | --- | --- |
| `capacity-projected` | ProjectedOutput | 20 ms | 118–280 ms (3/3) | **468–520 ms (5/5)** |
| `capacity-projected` | ResizeDispatch | 100 ms | 110–224 ms (3/3) | **416–452 ms (5/5)** |
| `chunk-64` | ProjectedOutput | 20 ms | 22–34 ms (4/5) | **225–273 ms (5/5)** |
| `chunk-64` | ResizeDispatch | 100 ms | not seen | **189–252 ms (5/5)** |
| `chunk-1` | ProjectedOutput | 20 ms | not seen | **60–82 ms (5/5)** |
| `128-active` | ProjectedOutput | 20 ms | not seen | **24.5–28 ms (5/5)** |
| `rate-40MiB` | ProjectedOutput | 20 ms | 27 ms (1/5) | not seen |
| `capacity-raw`, `chunk-64`, `stalled-sink` | CancelDispatch | 100 ms | 122–203 ms (1 each) | **not seen** |

Two separations fall out of having run both hosts, and neither was available
from one:

- **Every Linux miss is 5 of 5.** Perfectly reproducible on an idle machine.
- **Every single-trial macOS miss disappeared on Linux** — all three
  `CancelDispatch` rows and `rate-40MiB`. Those were laptop contention.

### The rows that matter most

**`128-active` is ADR 0004's headline G1 scenario** — 128 independent active
producers — and it misses ProjectedOutput on all five Linux trials while passing
on macOS. A macOS-only run would have reported G1's primary scenario as clean.

**`chunk-64` has no defence.** ADR 0004 permits latency to be missed if "an
explicit overload outcome identifies the rejected operation", which is arguably
what the deliberately unpaced `capacity-projected` case is. `chunk-64` is not
that: it is a paced 10 MiB/s workload at ADR-specified parameters, missing a
20 ms target by 13×. Sixty-four-byte writes are also what an interactive shell
actually produces.

The pattern across `chunk-1` (60–82 ms), `chunk-64` (225–273 ms) and
`chunk-65536` (no miss) points at per-write overhead rather than per-byte
throughput. That is a hypothesis this experiment does not test.

## Follow-up: chunk batching

**This section is a different revision from everything above.** It measures a
candidate fix, not the qualification matrix, and it re-ran 4 of 26 cases. The
verdict above stands.

**Revision:** `e01c704` plus the `chunk-batching` patch — `serve()` feeds up to
32 queued output chunks per worker run instead of one — plus path-naming
instrumentation in the load fixture. Applied as a working-tree diff, not a
commit on `main`; `diff_sha256` in
[`data/0005/linux-x86_64-batching.json`](data/0005/linux-x86_64-batching.json)
identifies it. **Run:** 2026-09-13, same Linux box as above, load average 0.16.
5 cases × 5 repeats, 60 s each after a 10 s warmup — same duration and repeat
count as the numbers it is compared against. `capacity-projected` was run
separately under `ulimit -n 65535`; the other four ran at the box default of
1024.

`ProjectedOutput` p99 against the 20 ms target:

| Case | Trials | Before (`e01c704`) | After batching |
| --- | --- | --- | --- |
| `chunk-64` | 5/5 pass | 225–273 ms | **0.1 ms**, every trial |
| `chunk-1` | 5/5 pass | 60–82 ms | **5.5–6.0 ms** |
| `attached` | 5/5 pass | no miss | 0.2–0.3 ms |
| `128-active` | 5/5 **fail to start** | 24.5–28 ms | see below |
| `capacity-projected` | 5/5 **still miss** | 468–520 ms | 255–331 ms, see below |

`chunk-64` `ResizeDispatch` moved from 189–252 ms to 86–100 µs against its
100 ms target. `chunk-1` is the worst surviving case; it is inside target, but
it is the row to watch if the batch bound is ever revisited.

### `capacity-projected` still misses, and its numbers are not percentiles

The matrix's largest miss is unfixed. All five trials miss `ProjectedOutput`
(255–331 ms against 20 ms) and `ResizeDispatch` (235–286 ms against 100 ms).
Batching roughly halves the figure; it does not bring it near target. The case
is deliberately unpaced (`rate=0`, saturation mode) and accepted 77–79 MiB/s.

**The reported p99 for this case is the maximum, not a 99th percentile, before
and after.** The latency histogram
(`crates/application/src/diagnostics/histogram.rs`) has 1025 buckets of 100 µs,
so it resolves to **102.4 ms**; anything above lands in the final bucket, and
`percentile_upper_us` deliberately returns the recorded maximum for any rank
falling there. In these trials **99.76–99.79 % of `ProjectedOutput` samples are
in that overflow bucket**, so p50, p95, p99 and max are all the same number.
The same is true of the 468–520 ms baseline above, which is far beyond the
ceiling and can only have come from the overflow bucket.

This is conservative by design, not a defect — it reports an upper bound rather
than a wrong number. But it means two things must be said plainly. The
before/after comparison for this case is **max-to-max**, not p99-to-p99. And the
honest statement of the miss is: *at least 99.7 % of projected-output samples
exceed 102.4 ms*, itself more than 5× the target, with the distribution above
that unresolved. The `chunk-64`, `chunk-1` and `attached` figures are
unaffected — they sit well inside the histogram's resolution.

**Whether ADR 0004's overload escape applies is left open here.** The clause
permits a latency miss where "an explicit overload outcome identifies the
rejected operation". This case does produce exact replay accounting — 1.2 GiB
of gap bytes against 5.2–5.4 GiB delivered, attributed per producer across all
64 ledgers — which satisfies the separate criterion that "any observer replay
loss has an exact cursor gap". A cursor gap identifies *evicted bytes*, not a
*rejected operation*. Reading one as the other is a judgement for whoever signs
the gate, and this experiment does not make it.

### `128-active` did not run, and that is a fact about the host

All five `128-active` trials died inside `Population::new` while spawning the
sessions — at the `runtime` checkpoint, before any measurement — with
`Error: Process(Io)` and exit 1. The box defaults to **`ulimit -n 1024`**, and
128 sessions do not fit in 1024 descriptors.

Two controls, both same case, same duration, same binary:

- **Raise the limit:** under `ulimit -n 65535` the trial passes, `ProjectedOutput`
  p99 3.5 ms.
- **Revert the patch:** with batching stashed and the descriptor limit left at
  1024, the trial fails identically — same error, same checkpoint. Batching does
  not change descriptor usage.

So this is host configuration, not a runtime defect and not a regression.

**But it contradicts the matrix above, and the record cannot say why.** The
Linux run at `e01c704` completed `128-active` 5 of 5 and reported 24.5–28 ms. On
the same box today that case cannot start at the default limit. The original run
must have had a higher `ulimit -n`, and **the harness does not record it** — not
in the identity block, not in `LOAD.md`. The baseline figures are not withdrawn;
they are, on this evidence, not reproducible from the record alone. Recording
`ulimit -n` per run is the fix, and it is cheap.

### What this follow-up does not establish

- **5 of 26 cases.** The 21 not re-run include `128-mixed`, the rate and
  observer sweeps, and every resource case.
- **Linux only.** No macOS re-run; the macOS `chunk-64` misses (22–34 ms, 4/5)
  have no after-number.
- **One sitting**, as with the original run. No cross-day repetition.
- The path instrumentation added for this run never fired — no path syscall
  failed in any of the 20 trials. It diagnosed nothing here.

## Limitations

- **The macOS dataset is incomplete: 115 of 130 trials**, from 15 harness
  failures — 9 `ProcessLookupError` from the `lsof` resource census losing a
  process, and 6 trial aborts. **`dominant` has zero completed macOS trials**,
  so the dominant-producer scenario has no macOS evidence at all. Linux
  completed 130 of 130 with no harness failures. This is a defect in the
  harness's macOS resource census, not in the runtime, and it is unfixed.
- **macOS latency figures are upper bounds.** The host was an interactive
  desktop under load. Its correctness results are unaffected; its latency tails
  should not be read as this machine's best case.
- **500 active sessions were not run.** `LOAD.md` places full 500-session
  qualification outside this bounded 128-session matrix as host-dependent.
- **Neither host is qualified.** Per ADR 0004 and
  [`docs/verification.md`](../verification.md), no target has a qualification
  pass. This is load evidence on two platforms, not a platform qualification.
- **One run per host.** Five trials per case, but a single sitting; no
  cross-day repetition.
- **Latency figures above 102.4 ms are maxima, not percentiles.** The histogram
  resolves to 1025 × 100 µs and reports the recorded maximum for any percentile
  landing in the overflow bucket. This affects every row in the miss table above
  reading over 102.4 ms — both `capacity-projected` rows, and the Linux
  `chunk-64` rows. See the follow-up for the measured overflow fractions.
- **The descriptor limit in force was not recorded.** See the follow-up above:
  `128-active` cannot start on the same box at `ulimit -n 1024`, so this run
  had a higher limit that nothing in the data identifies.

## What would close G1

1. Bring ProjectedOutput p99 within 20 ms for `128-active`, `chunk-64` and
   `chunk-1`, or record an explicit overload outcome identifying the rejected
   operation where the workload is genuinely beyond capacity. **Partly done:**
   chunk batching brings `chunk-64` to 0.1 ms, `chunk-1` to 5.5–6.0 ms and
   `128-active` to 3.5 ms on Linux, measured at five repeats for the first two
   (see the follow-up above). **`capacity-projected` is not fixed** — it still
   misses at 255–331 ms, roughly half its previous figure. macOS has no
   after-number, and the patch is not on `main`.
2. Fix the macOS `lsof` census failure and complete the macOS matrix, including
   `dominant`.
3. Run the remaining ADR 0004 items this matrix does not cover: the
   after-all-observers-detach case, split UTF-8/VT sequences and terminal
   queries under load, and an explicit fairness outcome.
4. Record `ulimit -n` in the harness identity block, and state the limit
   `128-active` requires in [`scripts/release/LOAD.md`](../../scripts/release/LOAD.md).
   Without it the 128-session rows cannot be reproduced from the record.
