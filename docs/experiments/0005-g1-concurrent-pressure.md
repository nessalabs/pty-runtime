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

**G1 is not signed off. Every accounting check this matrix runs passes on both
platforms; its latency criterion fails reproducibly on an idle host.**

**"Accounting" is the honest word, and it is narrower than "correctness."** What
the fixture checks is byte and gap accounting against absolute offsets,
processed offsets, and terminal-query counts. It does **not** compare the
projected state against an uninterrupted native reference under load, and it
does not exercise a fast and stalled observer on the *same* session. Fairness is
listed below as unrun. (Split UTF-8 and VT sequences and terminal queries *are*
exercised — see the closeout list for what the payload emits and how replies are
checked; what is missing there is the reference comparison, not the inputs.) A projection or isolation defect in any of those could
pass every trial recorded here. Read the correctness result as "the ledger
balances", not as "the projection is correct".

This verdict is for `e01c704`. A follow-up measurement of a candidate fix, at a
different revision, is recorded in [Follow-up: chunk
batching](#follow-up-chunk-batching) below; it does not change this verdict,
because it re-ran 5 of the 26 cases.

| | macOS arm64 | Linux x86_64 |
| --- | --- | --- |
| Trials completed | 115 of 130 | **130 of 130** |
| Accounting-check failures | **0** | **0** |
| Trials missing a latency target | 10 | **20** |
| Trials with no result, cause unrecorded | 15 | 0 |

The second column is the evidence that matters, because it was measured on a
dedicated machine doing nothing else and every miss on it repeats on all five
trials. That rules out transient load *on that host* as the explanation for the
Linux misses. It does not explain the difference between the two columns: the
hosts differ in operating system, CPU, core count and compiler all at once, no
same-host idle-versus-loaded control was run, and host load was not recorded
during the matrix.

## What was run

26 cases × 5 trials, 60 s each after a 10 s warmup, per
[`scripts/release/LOAD.md`](../../scripts/release/LOAD.md): the 64-session /
16-active / 10 MiB/s controlled reference in attached, detached,
stalled-observer, stalled-event-stream-sink and dominant-producer modes;
unpaced `capacity-projected` and `capacity-raw`; **128 independent active
producers** and 128-session mixed populations; rate, chunk, observer and grid
sweeps; 64 raw idle sessions; and raw/projected resource measurement at 1, 32
and 128 sessions.

**The resource cases ran; their measurements were not retained.** Those six
trials kept only CPU deltas, the idle-CPU verdict and the closing cleanup
census. The fixture's `budget`, `aggregate`, checkpoint and in-load
`physical_resources` events are absent from both platform artifacts, so RSS and
allocation peaks, thread and descriptor scaling, and the logical bounds at 1,
32 and 128 sessions **cannot be audited from this record** — ADR 0004 asks for
them and this experiment does not supply them. Listing the cases is not the
same as having the numbers.

Both hosts recorded the same source identity: `source_head e01c704`, clean tree
(`diff_sha256` is the empty-input SHA-256), 311 files inventoried.

**That identity is weaker than it looks, and the artifact says so itself.** The
committed data keeps only the file *count*, not the inventory hashes, and its own
`source_binary_link` field carries the warning that hashes alone do not prove
which source built the binary. No build record was retained for either host. A
binary built from a different tree, run later against a clean checkout, would
record exactly this. So read `e01c704` as the revision the harness reported, not
as a proven build-to-source link; retaining the build log is what would close
that gap.

| | macOS | Linux |
| --- | --- | --- |
| Platform | macOS 26.6, Darwin 25.6.0, arm64 | Ubuntu 24.04, Linux 6.8.0-117, x86_64 |
| Host | Apple M5, 10 logical CPUs, interactive desktop under load | 8 vCPU / 15 GiB dedicated cloud box, otherwise idle |
| rustc | 1.98.1 | 1.97.1 |

Both clear the 1.85 MSRV. The compiler differs between hosts and is recorded
rather than reconciled.

## Accounting: passes

No trial failed an accounting check on either host. 145.6 GiB (macOS) and
151.7 GiB (Linux) of delivered bytes were verified against their expected values
at absolute offsets. See the verdict above for what this does and does not
cover — in particular there is no reference-state comparison under load here.

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

- **Every Linux miss is 5 of 5**, on an otherwise idle machine. Reproducible,
  and not transient load on that host.
- **Every single-trial macOS miss is absent from Linux** — all three
  `CancelDispatch` rows and `rate-40MiB`. Each appeared once in five trials on a
  loaded interactive desktop and never on the idle box. **That is all this
  establishes.** Calling them noise, contention, or macOS-specific behaviour all
  go past the data equally: the hosts differ in OS, architecture, CPU, core
  count, kernel and compiler simultaneously, and no same-host
  loaded-versus-idle control was run. A one-off miss that does not reproduce on
  a different platform is undiagnosed, not dismissed.

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
candidate fix, not the qualification matrix, and it re-ran 5 of 26 cases. The
verdict above stands.

**Revision:** `e01c704` plus the `chunk-batching` patch — `serve()` feeds up to
32 queued output chunks per worker run instead of one — plus path-naming
instrumentation in the load fixture. It was measured as a working-tree diff, so
`diff_sha256` in
[`data/0005/linux-x86_64-batching.json`](data/0005/linux-x86_64-batching.json)
can verify those bytes only to someone holding them.

**The measured source is now committed**, as `56c013f` on the `chunk-batching`
branch (PR #18); the instrumentation is `bb4f000` on the same branch. Read the
numbers below against those two commits, **not** against that branch's head:
review of #18 added a failure-and-closure guard and a 256 KiB byte bound to the
batch afterwards, and neither was in this build. Neither changes these figures —
the fixture runs at the 4 KiB default `feed_bytes`, where the byte bound works
out to 64 chunks and the 32-chunk bound still binds first, and the guard only
engages on a failed or closing projection, which no trial here reached — but
that is reasoning, not a re-measurement. **Run:** 2026-09-13, the same Linux box
as above, load average 0.16 — but **not the same kernel throughout**; see the
limitation below.
5 cases × 5 repeats, 60 s each after a 10 s warmup — same duration and repeat
count as the numbers it is compared against. `capacity-projected` and
`128-active` were run under `ulimit -n 65535`; the other three ran at the box
default of 1024.

`ProjectedOutput` p99 against the 20 ms target:

| Case | Trials | Before (`e01c704`) | After batching |
| --- | --- | --- | --- |
| `chunk-64` | 5/5 pass | 225–273 ms | **0.1 ms**, every trial |
| `chunk-1` | 5/5 pass | 60–82 ms | **5.5–6.0 ms** |
| `attached` | 5/5 pass | no miss | 0.2–0.3 ms |
| `128-active` | 5/5 pass at raised limit | 24.5–28 ms | **8.0–9.1 ms** |
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

### `128-active`: passes once the host allows it to start

**At the box default of `ulimit -n 1024` it does not run at all.** All five
trials in the first sitting — the `g1-batching-4case` run in the data file,
whose five `128-active` entries each carry a `failure` record — died inside
`Population::new` while spawning the sessions, at the `runtime` checkpoint and
before any measurement, with `Error: Process(Io)` and exit 1. 128 sessions do
not fit in 1024 descriptors.

**Re-run at five repeats under `ulimit -n 65535`, it passes 5 of 5**:
`ProjectedOutput` p99 **8.0–9.1 ms** against the 20 ms target, `ResizeDispatch`
1.7–3.8 ms against 100 ms, holding the offered 10.0 MiB/s. These are real
percentiles — **zero samples in the overflow bucket**, against ~157 000 samples
per trial — unlike the capacity rows above. This is ADR 0004's headline G1
scenario, and at `e01c704` it missed on all five Linux trials at 24.5–28 ms.

An earlier single trial of this case reported 3.5 ms. The five-repeat figure of
8.0–9.1 ms supersedes it; one trial was not enough to characterise it.

Two controls, both same case, same duration, same binary:

- **Raise the limit** (`g1-128-ulimit`, then `g1-128-batching-x5`): under
  `ulimit -n 65535` the case passes; see the five-repeat figures above.
- **Revert the patch** (`g1-128-baseline`, **one trial**, deliberately): with
  batching stashed and the descriptor limit left at 1024, the trial fails
  identically — same error, same checkpoint. One trial is enough to show the
  failure does not depend on the patch, and is not offered as a rate.

So the descriptor limit is strongly indicated — but **the artifact cannot prove
it.** Each failing entry retains only `Process(Io)`, exit 1 and the `runtime`
checkpoint: no `NOFILE` value and no errno, because `ProcessError::Io` carries
no payload and the harness did not record the limit in force at the time. The
limit was raised in a later sitting and the shell reported it, which is not the
same as the artifact recording it. Read this as a well-supported hypothesis with
two controls behind it, not as a proven cause; PR #19 makes future runs record
the limit, and an errno on `ProcessError::Io` is what would close the rest.

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
- **The follow-up spans two kernels, and three of its comparisons cross one.**
  The box reported `6.8.0-117` for the baseline matrix, and reports it again for
  `128-active` (five repeats), `capacity-projected` and the revert control. It
  reported **`6.8.0-136`** for the run that produced `chunk-64`, `chunk-1` and
  `attached`, and for the single raised-limit `128-active` trial. Those three
  before/after comparisons therefore change the kernel along with the patch, and
  this was not one platform configuration or one sitting. The `chunk-64`
  movement is 225-273 ms to 0.1 ms, which no kernel revision plausibly accounts
  for, and the mechanism is understood — but *plausibly* is the word, and a
  same-kernel repeat is what would remove the confound. `128-active` and
  `capacity-projected` are unaffected: they ran on `6.8.0-117`, the same kernel
  as the figures they are compared against.
- **`128-active` and `capacity-projected` ran at a different descriptor limit**
  from the other three cases, and from the matrix above. The limit is now
  recorded per run by the harness; these runs predate that.
- **Linux only.** No macOS re-run; the macOS `chunk-64` misses (22–34 ms, 4/5)
  have no after-number.
- **One sitting**, as with the original run. No cross-day repetition.
- The path instrumentation added for this run never fired — no path syscall
  failed in any of the 20 trials. It diagnosed nothing here.

## Which gate owns this failure is not settled here

ADR 0004 puts "ordered input/output … pass **concurrent pressure tests**" under
G1, and "ordered parsing, replies, resize … and reference-state comparisons"
under G2. `ProjectedOutput` under concurrent pressure sits across that line: the
pressure clause is G1's, the projection path it measures is G2's.

One fact sharpens the question rather than settling it. **Every recorded
`RawOutput` target passes, including in the trials where `ProjectedOutput`
fails.** The process-and-bytes path — G1's own territory — meets its target
under exactly the pressure that the projection path misses it under.

This experiment does not reassign the verdict, because that is a reading of the
ADRs rather than a measurement, and it changes which gate blocks a release. It
is recorded here for whoever owns that decision. What is not in question is that
G1's own remaining work — fairness, observer isolation on a single session,
retained resource evidence, 500 sessions, the macOS matrix — is unfinished
regardless of how the projection latency is filed.

## Limitations

- **The macOS dataset is incomplete: 115 of 130 trials**, and **the committed
  artifact does not say why.** Each of the 15 incomplete entries stops after
  `runtime_options`: no failure record, exit status, stderr, timeout or closing
  census was retained. The split previously stated here — 9 `ProcessLookupError`
  from the `lsof` census plus 6 aborts — came from console output that was not
  kept, so it cannot be audited from this data and is no longer asserted.
  What the artifact does support: 15 trials produced no result, and the cases
  affected are `dominant` (5), `rate-40MiB` (4), `capacity-projected` (2),
  `rate-20MiB` (2), `capacity-raw` (1) and `stalled-sink` (1). **`dominant` has
  zero completed macOS trials**, so that scenario has no macOS evidence at all.
  Attributing all 15 to the census harness rather than the runtime is therefore
  a hypothesis this data cannot settle; re-running with the failure records
  retained is what would settle it. Linux completed 130 of 130.
- **macOS latency figures are upper bounds.** The host was an interactive
  desktop under load. Its correctness results are unaffected; its latency tails
  should not be read as this machine's best case.
- **Cross-host differences are not attributed.** The two hosts differ in OS,
  architecture, CPU, core count, kernel and compiler at once; no host-load
  measurement was recorded during the matrix, and no same-host
  loaded-versus-idle control was run. Any statement here about *why* a result
  differs between hosts is an observation that it differs, not an isolated
  cause. **Settling the macOS one-off misses needs a controlled repeat on the
  same macOS host**, idle, at the same revision — not a comparison against
  Linux.
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
   `128-active` to 8.0–9.1 ms on Linux, each at five repeats (see the follow-up
   above). **`capacity-projected` is not fixed** — it still misses at
   255–331 ms, roughly half its previous figure. macOS has no after-number, and
   the patch is not on `main`.
2. Establish why 15 macOS trials produced no result — the artifact records no
   cause, so the census harness is a suspect and not a finding — and complete
   the macOS matrix, including `dominant`.
3. Run the remaining ADR 0004 items this matrix does not cover: the
   after-all-observers-detach case, a canonical reference-state comparison under
   load, and an explicit fairness outcome. Split UTF-8/VT sequences and terminal
   queries are **not** in this list: the fixture's payload emits `ESC[H`,
   `ESC[6n`, SGR colour, wide and combining UTF-8 and an emoji every 4096 bytes,
   the 1-byte and 4093-byte chunk cases split those across writes, and every
   authoritative reply is matched against `payload::queries()`. What is missing
   is the comparison against an independent reference terminal under load, not
   the inputs.
4. Record `ulimit -n` in the harness identity block, and state the limit
   `128-active` requires in [`scripts/release/LOAD.md`](../../scripts/release/LOAD.md).
   Without it the 128-session rows cannot be reproduced from the record.
