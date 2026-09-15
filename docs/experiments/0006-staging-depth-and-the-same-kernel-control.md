# Experiment 0006: staging depth, and the control Experiment 0005 could not run

**Gate:** G1 (process and bytes), [ADR 0004](../adr/0004-integration-and-release-qualification.md) ·
**Recording rules:** [ADR 0002](../adr/0002-performance-and-stability.md) ·
**Data:** [`data/0006/`](data/0006/)

## Why this exists

[Experiment 0005](0005-g1-concurrent-pressure.md) left two things open that its
own data could not settle.

Its follow-up measured `chunk-64`, `chunk-1` and `attached` on kernel
`6.8.0-136` against a baseline taken on `6.8.0-117`, so three before/after
comparisons changed the kernel along with the patch. And `capacity-projected`
missed its latency target by more than 13× with no explanation beyond "the
workload is unpaced".

Both are answered here, on one host, at one kernel, against the merged runtime
rather than a working-tree patch.

## Host and revision

Linux x86_64, 8 vCPU, kernel **`6.8.0-117`** — the same kernel as Experiment
0005's baseline — running `main` at `981a74a`, which contains the chunk-batching
change and the three corrections review added to it afterwards. Five trials per
case, 60 s each after a 10 s warm-up, `ulimit -n 65535`. Load average below 0.3
throughout.

## Part 1: the kernel confound, removed

Same case, same duration, same repeat count, now also the same kernel:

| Case | `e01c704` baseline (117) | Follow-up (136) | **This run (117)** | Target |
| --- | --- | --- | --- | --- |
| `chunk-64` | 225–273 ms | 0.1 ms | **0.8–0.9 ms** | 20 ms |
| `chunk-1` | 60–82 ms | 5.5–6.0 ms | **7.7–9.1 ms** | 20 ms |
| `attached` | no miss | 0.2–0.3 ms | **0.6–0.7 ms** | 20 ms |

**The confound was real and the earlier figures were optimistic.** `chunk-64` is
0.8–0.9 ms on the baseline's own kernel, not 0.1 ms — roughly eight times
larger than reported. The conclusion is unchanged and still large: a 280×
improvement, comfortably inside target on all five trials. But "0.1 ms" was
partly the kernel, and Experiment 0005's follow-up table should be read against
this row.

## Part 2: `capacity-projected` is a queue depth, not a defect

`capacity-projected` is the unpaced saturation case and the largest miss in the
matrix. Its `staging_slots` budget peaked at 4096 against a global limit of
16384 — exactly 16 active sessions at the 256-slot per-session cap. Every active
queue was full while the global budget sat at a quarter, which suggested the
tail was queue residence time rather than scheduling overhead.

Sweeping that depth, five trials each:

| slots | ProjectedOutput p99 | Passes 20 ms | p50 | Accepted | Fixture RTT p99 | Replay gap |
| ---: | --- | --- | ---: | ---: | ---: | ---: |
| **256** (default) | 271.6–289.1 ms | **no** | 278.7 ms | 76.1 MiB/s | 104.5 ms | **1.46 GiB** |
| 64 | 66.0–70.9 ms | no | 54.1 ms | 74.6 MiB/s | 10.8 ms | 0 |
| 32 | 33.1–35.7 ms | no | 26.2 ms | 77.8 MiB/s | 10.6 ms | 0 |
| **16** | **18.4–19.4 ms** | **yes, 5/5** | 13.3 ms | 78.5 MiB/s | 8.8 ms | 0 |

`ResizeDispatch` follows the same curve and passes its 100 ms target from 64
slots down.

**Latency tracks depth almost exactly**, at roughly 0.83 ms per slot across the
three measurable points. The 256-slot row reads higher per slot because its
figure is not a percentile: 99.79 % of its samples exceed the histogram's
102.4 ms ceiling, so p50, p99 and max collapse to the same number. **Below 256
slots the overflow is zero**, so those rows are real percentiles — which is also
independent evidence that the distribution genuinely moved rather than the
instrument losing sight of it.

### What the shallower queue does *not* cost

[`LOAD.md`](../../scripts/release/LOAD.md) warns that a lower internal output
p99 can shift waiting upstream, and asks for producer backpressure, fixture RTT
and accepted throughput alongside any such comparison. They are above, and none
of them degrades:

- **Throughput is unchanged** — 76.1 MiB/s at 256 slots, 78.5 MiB/s at 16.
- **Control round-trip improves twelve-fold** — 104.5 ms to 8.8 ms. The waiting
  did not move upstream; it stopped happening.
- **Replay eviction stops entirely** — 1.46 GiB of gap bytes at the default,
  zero at 64 slots and below. The deep queue was *causing* the eviction that
  Experiment 0005 recorded as accounted-but-lossy.

So the default trades 20× projected-output latency, 12× control latency, and
1.46 GiB of evicted replay, for no measurable throughput.

### This is the shipped default, not a fixture setting

`ProjectionOptions::new()` sets `staging_slots: 256`
(`crates/domain/src/projection/options.rs`). The fixture inherits it rather than
inventing it.

**This experiment does not change it**, and the decision has since been taken
to leave it: 278 ms under unpaced saturation is accepted for now. Recorded here
so the number is not rediscovered as a defect later.

One workload — 16 unpaced producers at 4093-byte chunks — was never a basis for
a product default anyway, and a deeper queue is exactly what absorbs bursts
without backpressuring a producer. A six-workload comparison at 256 against 16
was started and stopped once the decision was made.

### Six workloads at 256 against 64: a trade, not a free win

The saturating case made a shallower default look free. It is not. Six cases,
five trials each, at both depths, on the same host and kernel — `ProjectedOutput`
p99, worst of five:

| Case | @256 | @64 | |
| --- | ---: | ---: | --- |
| `attached` | 0.7 ms | 0.7 ms | unchanged |
| `chunk-64` | 0.8 ms | 0.9 ms | slightly worse |
| `128-active` | 14.5 ms | **8.9 ms** | better |
| `stalled-observer` | 0.7 ms | 0.8 ms | slightly worse |
| **`dominant`** | **0.8 ms** | **1.7 ms** | **worse, about twofold** |
| `rate-40MiB` | 1.1 ms | 1.0 ms | unchanged |

Accepted throughput is identical at both depths in every case, every latency
target passes at both, and no case gains replay eviction at 64 —
`stalled-observer` evicts the same 0.67 GiB either way.

**The `dominant` regression is real, not scatter.** The five trials do not
overlap: 0.6–0.8 ms at 256 against 1.0–1.7 ms at 64. `chunk-64` and
`stalled-observer` move the same way by 0.1 ms with almost no overlap. One
heavy producer is exactly the shape a deep queue absorbs, so this is the cost
that the single saturating workload could not show.

**The default stays at 256**, on a criterion fixed before the numbers were
seen: no case may have a worse p99 at 64 than at 256. `dominant` does.

The wider reading matters more than the verdict. **Across every paced workload
the depth barely matters** — everything lands between 0.6 ms and 14.5 ms
against a 20 ms target at both depths. Depth dominates only under unpaced
saturation, where 64 would not be enough either (66 ms against 20 ms; that case
needs 16). So there was less to gain here than the first sweep implied, and a
measurable amount to lose.

**One consequence is worth separating from the latency**, because accepting the
latency does not automatically accept this: at 256 slots the saturating case
evicts **1.46 GiB of replay**, and at 64 and below it evicts none. That loss is
exactly accounted — every observer gets a precise cursor gap, which is what ADR
0004 requires — so it is correct behaviour, not corruption. But it is bytes an
observer cannot get back, caused by the queue depth rather than by the offered
load. If that is also acceptable, nothing here needs revisiting.

## An invalid run happened first, and is why the fixture now refuses unknown flags

The first sweep returned flat: no change in p99 from 256 slots down to 16. It
was wrong. The driver builds flags from configuration keys and sent
`--staging_slots`; the fixture reads `--staging-slots`, did not find it, and
fell back to its default. **All four runs were the same configuration.**

It was caught only because the fixture echoes its effective slot count in the
`start` record. The run completed, every trial passed its accounting, and the
flat result looked like a clean refutation of the hypothesis it was testing.

The fixture now refuses any argument outside the names it knows. A harness that
quietly substitutes a default for what it was asked to run cannot be trusted to
have run the experiment being reported.

## Part 3: after all observers detach

ADR 0004 asks what happens once *every* observer has gone while producers keep
running. Nothing in the matrix exercised it. `detached` never attaches an
observer at all, which is a different question: it never crosses the transition,
and the transition is where retention, parsing and admission have to notice that
nobody is reading any more.

`detaching` attaches observers normally, then drops all of them half way through
the measurement phase and keeps producing. Five trials:

| | Result |
| --- | --- |
| Observers released | 64, at 30.0 s, every trial |
| Accepted throughput after they left | **10.0 MiB/s** — the offered rate, unchanged |
| `ProjectedOutput` p99 | 0.6–0.8 ms against a 20 ms target, passing |
| Replay gap seen by a later attach | 684.1 MiB, **exactly accounted** in all five |
| Final process tree / zombies | 1 / 0 |

**The gap is the finding, not a failure.** The run's final accounting attaches a
fresh observer at offset zero. A 1 MiB per-session retention cap cannot still
hold thirty seconds of output at 10 MiB/s, so that observer must be told it
missed something — and it is told *exactly* what, with `verified + gaps ==
total` holding on every trial. That is ADR 0004's requirement: a later attach
gets a precise cursor gap rather than silence or the wrong bytes.

The first run of this case failed, on an assertion of mine rather than on the
runtime: `detaching` had been grouped with `attached` in a check that no
observer falls behind. A re-attached observer necessarily falls behind. The
assertion was wrong, the runtime was right, and the exact-accounting assertion
that every mode already shares is what actually proves the claim.

## Part 4: fairness, as an outcome rather than an inference

ADR 0001 states the clause: "a flooding session or slow snapshot/event consumer
does not starve input, cancellation, resize, or other sessions", and ADR 0004
asks for it as an **explicit outcome**. Aggregate throughput cannot supply one —
one session taking everything and another taking nothing sums to exactly the
same total as an even split. The fixture already tracked per-session bytes;
nothing reported their distribution.

Three trials each, measurement phase, bytes per active producer:

| Case | Active | Min | Median | Max | Max/median | Starved |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `128-active` | 128 | 4.7 MiB | 4.7 MiB | 4.7 MiB | 1.000 | 0 |
| `attached` | 16 | 37.5 MiB | 37.5 MiB | 37.5 MiB | 1.000 | 0 |
| **`dominant`** | 16 | **4.0 MiB** | 4.0 MiB | **540.0 MiB** | **135×** | **0** |

`dominant` is the case that tests the clause. One session floods at 135 times
the median, and the other fifteen are served *identically to each other* —
min over median exactly 1.000, with nothing starved, on every trial.

**No threshold was invented.** ADR 0002 sets no fairness number, so a pass mark
defined in the fixture would be one this project never agreed. The distribution
is reported so it can be read against whatever target is chosen, and the single
asserted property is the one that needs no threshold to justify: a producer
asked to produce, in a phase that delivered bytes, must not have been served
none of them.

**The flooder is excluded by identity, not by size.** The fixture also reports
the min-over-median of the *non-dominant* producers alone, and that figure first
dropped whichever producer happened to be largest. In the measured runs those
are the same producer — `rate_for` gives producer 0 nine tenths of the offered
rate, and it took 540.0 MiB of the 600 — so no number here changes. They stop
being the same in the one case the figure exists to expose: producer 0
underperforming enough not to be largest, where dropping the largest excludes an
innocent producer and leaves the flooder inside the population the ratio claims
to describe. It is reported as `null` in modes that have no dominant producer,
rather than as a ratio of a population that does not exist.

**What this does not measure.** The non-dominant producers are rate-limited, so
4.0 MiB each is their offered rate being met rather than a contested share
they had to win. It establishes that a flooding session does not prevent others
reaching their rate — which is the clause — but it is not a measurement of
contention for a scarce resource, and it says nothing about input, cancellation
or resize fairness, which the same clause also names and which this case does
not isolate.

## Part 5: reference-state comparison under load

ADR 0004 asks that reference-state comparisons pass. Unit tests already feed an
independent engine the same bytes and require an equal view, but only on a quiet
runtime. What that leaves unproven is the thing this matrix exists to stress:
whether the projection is still *correct* after pressure, rather than merely
still keeping up.

The new `reference` case runs the controlled reference workload and then, for
one session, rebuilds its entire byte stream from `payload` — a pure function of
offset and producer, so it can supply history that the 1 MiB retention cap
cannot — feeds it to an independent `GhosttyTerminalFactory` engine, and
compares that engine's view against `session.projected_view()`.

| Trial | Bytes compared | Feed time | Grid | Views equal | Replay gap |
| --- | ---: | ---: | --- | --- | ---: |
| 1 | 43.8 MiB | 0.23 s | 80×24 | **yes** | 0 |
| 2 | 43.8 MiB | 0.22 s | 80×24 | **yes** | 0 |
| 3 | 43.8 MiB | 0.22 s | 80×24 | **yes** | 0 |

It is affordable because ADR 0002's offered rate is *combined* across producers,
so one producer's stream is about 44 MiB rather than the ~600 MiB the population
moves between them. The comparison is an assertion rather than a reported
number: an unequal view after load is a correctness failure, not a measurement.

**What it does not cover.** One session of the 64, one grid, the controlled
reference rate — not saturation, not the small-chunk cases, and not after a park
or restore. Equality is checked once, at the end of the measurement phase,
rather than continuously.

**It compares bytes, not bytes *and controls*, which is what ADR 0004 asks
for.** The reference engine is fed the regenerated byte stream and nothing else.
The pressure phase does issue resizes every 100 ms, but they are *same-grid*, so
an implementation that dropped every one of them under load would still produce
an identical 80×24 view and pass this comparison. `tests/projected_runtime.rs`
does not close the gap either: it applies its resize after feeding the bytes,
so it never exercises the interleaving. Making this a controls comparison needs
the resize to vary the grid and the offset at which each took effect to be
replayable into the reference engine — neither of which this run has.

## Part 6: 500 sessions, and the ceiling that stops them being projected

ADR 0002 asks for 500 sessions "qualified on a host with sufficient PTY/process
capacity", and explains why that had not happened: the macOS host reports a
system-wide PTY limit of 511 which must also serve everything else. The Linux
box reports **4096**, so it is the host that requirement describes. The fixture
capped itself at 128 sessions, so the cap was raised to 512 — the fixture must
allow what the ADR asks for; whether a host can carry it is what a run finds
out.

**Raw, 500 sessions: passes.** One trial, 60 seconds after warm-up,
`ulimit -n 65535`:

| | |
| --- | --- |
| Processes at steady state | **1,501** |
| Resident memory | 3,220.2 MiB |
| Descriptors | 14,508 |
| Threads | 2,507 |
| Idle CPU | **0.54 %** against a 1.0 % target |
| After close | tree of 1, **zero zombies** |

**Projected, 500 sessions: refused, before a single session starts**, with
`Error: Projection(Capacity)`. That is not a defect and not a host limit. Each
projected session reserves 8 MiB of native memory, and `ProjectionLimits`
defaults `resident_bytes` to 1 GiB — which permits exactly **128** projected
sessions.

That number is the matrix's own 128-session ceiling, arrived at independently.
The bounded matrix stops where the default resident quota stops, which is worth
knowing: 128 was not a cautious guess, it is what the shipped defaults allow.

Raising it is a decision rather than a fix. 500 projected sessions would reserve
about 3.9 GiB before any output is parsed, and the refusal is already the
behaviour the runtime promises — a typed, immediate `Capacity` rather than a
degraded run. What this experiment establishes is the capacity claim for the raw
path at 500, and the exact reason the projected path does not reach it.

**Neither 500-session case belongs in the default matrix**, and both were
briefly put there while this was being measured. The raw one needs a host with
1,501 processes and ~14,500 descriptors; the projected one cannot pass anywhere
on the shipped defaults, for the reason just given. Selecting them by default
made the documented `load.py --output ...` command record a failed trial and
exit non-zero on every host, which turns the run's own pass bit from a result
into noise. They are now run by name — `--case resources-raw-500` — and `--list`
reports them under `host_dependent`.

## Part 7: what a long run is actually for

ADR 0002 asks the 12-hour soak for "stable resource plateaus after warm-up, no
accumulating children/descriptors/workers". Nothing here had ever tested how
much of that a shorter run can answer, so the question "why twelve hours" had
only a reasoned answer. This is the measured one.

A 55-minute `attached` trial, 632 periodic samples inside the measurement
window, analysed with `scripts/release/accumulation.py`. The extrapolation
stretch is **1.09×** — an hour projected from 55 minutes — where a 60-second
trial projects at 60× and its per-hour figures are not evidence of anything.

Each row is fitted twice: over the **cohort** of 193 processes measurable in
every one of those censuses, whose population is identical between samples by
construction, and over the whole-tree totals, which are the only basis that can
see a leak in the process churn itself. The cohort figures are the ones below;
where the two disagree, both are given.

| Resource | Growth over 55 min | Per hour | Verdict |
| --- | ---: | ---: | --- |
| Descriptors | +0.07 | +0.08/h | flat — 1,864 throughout, peak 1,873 |
| Threads | +0.01 | +0.01/h | flat — 327, peak 328 |
| Processes | +0.0007 | +0.0007/h | flat — 193, peak 197 |
| Resident memory | +45.8 KiB | 50.0 KiB/h | real, immaterial |

**Nothing accumulates.** The memory slope is statistically real — significance
**15.6** — and still **0.0108 % per hour** against a 451 MiB working set.
Projected across the full twelve hours that is **0.59 MiB, 0.13 %**. This is
exactly the separation the tool exists to make: a slope can be certain and
irrelevant at once, so it is reported as "moving but too little to matter"
rather than as a leak.

**The two bases disagree on memory, and the fixed population is the sharper
one.** Over the whole-tree totals the same slope is *not* distinguishable from
zero at all — significance 0.62, verdict "flat" — because the transient probe
churning through the tree adds and removes a few hundred kilobytes at a time and
buries a 46 KiB drift in the noise. Restricting the fit to a population that
does not change resolves it. The cohort was added to stop turnover **masking**
growth; on this run it also stops turnover masking a slope that is real and
harmless, which is the same effect pointing the other way.

Two earlier runs of this case are superseded rather than corrected, because each
is a different run rather than a different arithmetic: +113 KiB at 0.0286 %/h
(significance 3.6, 424 MiB base), from an artifact predating the cohort entirely;
and +69.4 KiB at 0.0166 %/h (significance 20.6, 445 MiB base), from a cohort
keyed on pid alone. The first rested on whole-tree totals guarded only by a
process *count*, which cannot tell a costly process leaving and a cheap one
arriving from nothing happening. The second could not tell a recycled pid from
the process that held the number before it. The figures above are from a cohort
keyed on `(pid, start_ticks)`. All three agree on the conclusion, and on the
order of magnitude: under a megabyte over twelve hours.

### The part that was got wrong

The argument had been that a soak's only remaining job is catching a slope whose
onset is later than a short window — a narrow risk, easy to discount.

The first attempt at the original run **died after 91 seconds**, on a `PermissionError`
reading `/proc`. The census walks a `ps` snapshot, this fixture spawns a
transient cancel probe every second, and over a long run the kernel hands one of
those numbers to a process that is not ours. Reading its memory is then
correctly refused. Every 60-second trial in the matrix passes over it.

So a long run does not only measure slopes more accurately. **It reaches states a
short one cannot** — pid reuse, counter rollover, the results of sustained churn
— and those arrive as outright failures rather than as gradients. That is a
stronger case for a soak than the slope argument, and it came from the runtime
rather than from reasoning about it.

### What this means for the cadence decision

Leak detection is now cheap: about an hour, with the slope check, bounds
accumulation with a trustworthy extrapolation. The twelve-hour run earns its
place on the *durability* class of failure — the kind that found the census
defect above — rather than on finding leaks. That is a schedulable trade with
numbers attached, rather than a number nobody could justify.

It does not make the soak redundant, and this experiment does not claim a
twelve-hour result. A 55-minute window cannot see an onset at hour six.

## Part 8: the overload outcome was always counted, never reported

ADR 0004 permits a latency miss where "an explicit overload outcome identifies
the rejected operation". That had been recorded as unexercised, and I had
described supplying it as a change to what the runtime promises. **That was
wrong.** The runtime has counted it all along — `CounterKind::InputSaturation`
is admission rejections, `CounterKind::OutputBackpressure` is lossless admission
attempts that had to wait — and the counters reached the record only as
unlabelled positions inside a Debug string, which is not something a gate can be
evaluated against.

One trial each, counters named:

| Case | Admission rejections | Backpressure events | Observer gaps | Gap bytes | Any operation refused or delayed | `ProjectedOutput` p99 |
| --- | ---: | ---: | ---: | ---: | --- | ---: |
| `attached` | 0 | 0 | 0 | 0 | **no** | 0.7 ms, passes |
| `capacity-projected` | 0 | **5,843,224** | — | **0.86 GiB** | **yes** | fails |

The case that meets its target emits no overload signal at all; the case that
misses emits millions of backpressure events and 0.86 GiB of exactly-accounted
replay loss. The signal and the failure coincide precisely.

These counters are run-to-run quantities, not constants: earlier runs of the
same case recorded 5,645,046 and 1.27 GiB, and 5,868,638 and 0.86 GiB. The figures above are the ones in the
committed artifact, which is the only version of them that can be checked.

**The crux is that admission rejections are zero.** Nothing is *rejected* under
saturation. Operations are delayed losslessly, and replay is evicted with an
exact cursor gap per observer. ADR 0004's escape names a *rejected operation*,
so whether backpressure-and-gaps satisfies it is a reading of that ADR.

This experiment does not make that reading, and deliberately: it decides whether
a gate passes. What it changes is that the question can now be answered from the
artifact instead of from the source, which is what "explicit outcome" has to
mean if it means anything.

## Part 9: the macOS matrix, completed

Experiment 0005 lost 15 of 130 macOS trials to the census raising when a
sampled process exited, and left `dominant` with no macOS evidence at all. With
that fixed, the six cases that had lost trials were re-run on macOS arm64,
five trials each:

| Case | Trials | Passed | Censuses that saw a process vanish | Latency misses |
| --- | ---: | ---: | ---: | --- |
| `dominant` | 5 | **5** | 3–4 per trial | none |
| `capacity-projected` | 5 | **5** | 15 | ProjectedOutput 75.7–83.5 ms |
| `capacity-raw` | 5 | **5** | 11 | none |
| `rate-20MiB` | 5 | **5** | 16 | none |
| `rate-40MiB` | 5 | **5** | 17 | **ProjectedOutput 81 ms** |
| `stalled-sink` | 5 | **5** | 17 | none |

**Thirty trials, none lost, while the race fired 93 times.** That
is the point: the census still meets processes that have exited, at the same
rate as before. It simply records them now instead of discarding the sample.
This is the fix demonstrated under load on the platform that lost the trials,
rather than against a synthetic exit.

### `rate-40MiB` missed again, and the per-trial numbers say why

Experiment 0005 recorded one `rate-40MiB` miss on macOS, absent from Linux.
This re-run missed once more, at 81 ms against a 20 ms target — and the first
reading written here was that two independent macOS-only occurrences are "no
longer well described as noise". **The per-trial figures do not support that**,
and they were available before the claim was made:

| Trial | `ProjectedOutput` p99 | Owner CPU | Accepted |
| ---: | ---: | ---: | ---: |
| 1 | 0.3 ms | 180 % | 40.0 MiB/s |
| **2** | **81.2 ms** | **250 %** | 40.0 MiB/s |
| 3 | 0.4 ms | 204 % | 40.0 MiB/s |
| 4 | 0.4 ms | 210 % | 40.0 MiB/s |
| 5 | 0.7 ms | 260 % | 40.0 MiB/s |

Four of five trials are **thirty times inside** the target, with the offered
rate held in all five. A platform-specific defect does not leave four trials at
0.3–0.7 ms and spike one to 81 ms; a transient stall on an interactive desktop
running at 180–260 % owner CPU does exactly that. Both macOS occurrences are one
miss in five, and Linux has none.

So the original reading stands: this is most consistent with **host contention**,
which is what Experiment 0005 said and what its "macOS latency figures are upper
bounds" limitation already covers. It is not proven — nothing here isolates
contention — but "two occurrences, therefore not noise" counted events without
looking at their distribution, and the distribution is the evidence.

`capacity-projected`'s 75.7–83.5 ms is the saturation behaviour described in
Part 2 and is expected rather than new.

### A build that could not run the case

`stalled-sink` failed all five trials on the first attempt, with
`stalled-sink requires --features event-stream` — a binary built with `ghostty`
alone, against a `LOAD.md` that specifies `event-stream` for this matrix. The
fixture refused loudly rather than running something else, which is the
behaviour added in Part 5's companion change; re-run with the documented
features it passes 5 of 5.

## The stalled-publisher outcome, now that it is retained

ADR 0004 asks that a slow snapshot/event consumer not starve anything, and
`stalled-sink` is the case that holds a real `event-stream` publisher stalled
mid-publication. It emits the proof as one record — how many times the sink was
called, how many publications it held in flight, and how large the largest
payload it was asked to buffer grew. **The archiver was dropping that record**,
so every archived `stalled-sink` trial held the case's name and none of its
evidence. Three trials on the box, with it retained:

| Trial | Sink calls | Held in flight | Largest payload | `ProjectedOutput` p99 | Replay gap |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 1 | **1** | 4,130 B | 0.7 ms | 0 |
| 2 | 1 | **1** | 4,130 B | 0.7 ms | 0 |
| 3 | 1 | **1** | 4,130 B | 0.6 ms | 0 |

Exactly one publication is held, its payload stays within one chunk rather than
growing without bound, and the projected path meets its target while the
publisher is stalled, with every byte still accounted. That is the clause, and
it is now checkable from the artifact instead of from the fixture's source.

## A note on the artifacts themselves

Artifacts carry `retention_version`, so which policy produced one is a field
rather than a guess, and three policies are represented here:

| Artifact | Version | Why |
| --- | ---: | --- |
| `fairness`, `500-sessions`, `after-all-observers-detach`, `overload-outcome`, `reference-state`, `soak-55min`, `stalled-sink` | **7** | re-run on the box with the identity-checked collector |

Every figure quoted from a version-7 artifact was re-read from it after the
re-run rather than carried over; several moved by a fraction of a percent, as
independent runs of the same case do, and each is now the number in the
committed file.
| `macos-arm64-rerun` | 3 | its raw run no longer exists on that machine |
| `same-kernel`, `staging-sweep`, `depth-by-workload` | unstamped | predate the version field; raw runs lost when the box stopped |

Version 7 names processes by incarnation rather than by pid. Census rows carry
`start_ticks` and `ppid`, the collector refuses a process whose identity changed
mid-read, and the cohort is keyed on `(pid, start_ticks)` — so a recycled number
leaves the cohort instead of contributing its new occupant's memory under the
old one's name. It also counts every once-per-trial record and refuses any that
appears twice.

Version 6 keeps the per-PTY progress and blocking report — one row per producer
per phase, carrying bytes, blocked-write duration, maximum backpressure wait,
write calls, EAGAIN and partial-write counts — which ADR 0004 asks for,
`LOAD.md` said was retained, and the archiver had been dropping entirely.
Nothing else in an artifact can reconstruct it: aggregate throughput and the
fairness extrema give totals and endpoints, not which PTY waited. It also keeps
the `stalled_sink` outcome, resize and cancel failures, and the runtime options.
It roughly quadruples an artifact, which is the cost of the evidence.

Version 5 records how many terminal records each trial file held, so a later
record cannot overwrite an earlier contradictory one unnoticed. Version 4 adds
the stable-process cohort described in Part 7. Version 3 added per-case outcome
events, checkpoint records and final budget values. The unstamped three have
none of those.

**This does not affect any figure cited above from them.** Parts 1 and 2 rest on
latency targets, throughput and ledger totals, which those artifacts do retain,
and every figure in them was re-read from the artifact for this revision. What
they cannot support is the allocation-peak and logical-cleanup analysis the
newer policy preserves — so that analysis is not attempted from them.

`macos-arm64-rerun` at version 3 carries no cohort series, so Part 9's
resource claims rest on the whole-tree totals with the process-count guard.
`accumulation.py` reports nothing accumulating in any of its 30 trials on that
basis; the sharper basis is not available for it.

## Limitations

- **One host, one kernel, one sitting.** No macOS, no repetition across days.
- **The sweep is one workload.** 16 active unpaced producers at 4093-byte
  chunks and 64 resident sessions. Nothing here measures a bursty or
  paced workload, which is where a deep queue would be expected to earn its
  cost.
- **Only four depths**, at powers of two from 256 to 16. The lowest passing
  depth was not bisected; 16 passes at 18.4–19.4 ms against a 20 ms target,
  which is inside but not by much.
- **The 256-slot latency figures are maxima, not percentiles**, for the reason
  given above. The rows below it are percentiles.
- **Source identity is the revision the harness reported.** As in Experiment
  0005, no build record binds the measured binary to `981a74a`.
- **The resource cases in part 1 ran but are not analysed here.** Their series
  are retained in the artifact for the first time. Running
  `scripts/release/accumulation.py` over them reports nothing accumulating in
  any of the 25 trials, which bounds growth within each 60-second window; it
  does not bound a slope whose onset is later than that window.
- **The detach case covers one shape.** All observers leave at once, half way
  through, at 64 sessions with one observer each. Staggered departures, partial
  detachment, and detaching under saturation are not covered.

## What this closes, and what it does not

It closes the kernel confound, and it explains `capacity-projected` — the last
unexplained latency failure in the G1 matrix — as a configured queue depth with
a measured cost curve, rather than a defect.

**It does not sign off G1**, and the list of what was open has changed rather
than emptied. Fairness, the after-all-observers-detach transition, a
reference-state comparison under load, 500 sessions and the six lost macOS
cases all have runs here — Parts 3, 4, 5, 6 and 9 — where before they had none.

What remains open after them:

- **The 12-hour soak itself has not been run.** Part 7 bounds accumulation over
  55 minutes and argues the soak's remaining value is durability rather than
  leak detection; it does not stand in for one.
- **The reference-state comparison does not replay ordered controls.** It feeds
  the same bytes to an independent engine, and the resizes the pressure phase
  issues are same-grid, so an implementation that dropped every resize under
  load would still produce an equal view and pass. See Part 5.
- **500 sessions are qualified on the raw path only.** The projected path is
  refused by the shipped resident quota, which is behaviour rather than a
  result, and 500 sessions have not been run on macOS at all.
- **The macOS matrix is complete only for the six re-run cases**, and its
  latency figures remain upper bounds on a contended host.
- **Fairness is reported, not thresholded**, and covers neither input,
  cancellation nor resize fairness — which the same ADR clause names.
- **Whether projected-output latency is G1's to answer at all** is filed under
  G2 on this ADR's own wording, with the counter-argument recorded. Nothing
  here signs that off either.
