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
| `ProjectedOutput` p99 | 0.60 ms against a 20 ms target, passing |
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
| 1 | 43.8 MiB | 0.25 s | 80×24 | **yes** | 0 |
| 2 | 43.8 MiB | 0.27 s | 80×24 | **yes** | 0 |
| 3 | 43.8 MiB | 0.25 s | 80×24 | **yes** | 0 |

It is affordable because ADR 0002's offered rate is *combined* across producers,
so one producer's stream is about 44 MiB rather than the ~600 MiB the population
moves between them. The comparison is an assertion rather than a reported
number: an unequal view after load is a correctness failure, not a measurement.

**What it does not cover.** One session of the 64, one grid, the controlled
reference rate — not saturation, not the small-chunk cases, and not after a park
or restore. Equality is checked once, at the end of the measurement phase,
rather than continuously.

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

It does not sign off G1. Fairness, the after-all-observers-detach case, a
canonical reference-state comparison under load, 500 sessions, and the
incomplete macOS matrix are all untouched, as is the open question of whether
projected-output latency is G1's to answer at all.
