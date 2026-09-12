# Experiment 0004: What bounded reader handoff would cost

Executed 2026-09-12 on one macOS arm64 host. Twenty cases × five repetitions,
case order reversed on alternate repetitions, from one experiment source.

This is the measurement spike that
[ADR 0003](../adr/0003-session-parking-and-state-transfer.md) requires before
anyone implements dynamic reader placement, and that
[ADR 0004](../adr/0004-integration-and-release-qualification.md) names as
needing "its own platform proof and comparative measurements". It prices a
placement policy. **It does not implement one, and nothing in this report
describes runtime behaviour.** The library still runs one dedicated reader per
live PTY for the whole life of a session — the accepted default from
[Experiment 0003](0003-cross-platform-concurrent-workloads.md) — and no file
under `crates/*/src` was changed to produce these numbers.

## Environment and reproducibility

| Host | CPU / memory | OS / pages | Rust |
| --- | --- | --- | --- |
| macOS arm64 | Apple M5, Mac17,3; 10 logical CPUs; 24 GiB | Darwin 25.6.0 (macOS 26.6, build 25G72); 16 KiB | 1.98.1 |

Base revision `d714e01`; the fixture source is the tree committed on this
branch. Source manifest SHA-256:
`962c930918c7c73057d104706d7c7b6ceb32c2a472a6675d395375edd0ac0555`. Fixture
binary SHA-256: `2024e96b7b288d376272a5f23b3b581c8e5929db78320951d7f2f2205992d06b`.
Descriptor limit 1048576; system-wide PTY limit 511, unchanged.

```sh
python3 experiments/run.py run --profile handoff --suite pty --output work/handoff
```

**Competing load, stated up front.** This is an interactive desktop, not a quiet
benchmark host. Load average at run start was 7.41 / 6.60 / 4.80 over 10 logical
CPUs; `searchpartyd` alone was observed at about 0.9 of a core, with a window
server and several Electron applications also resident. Memory and thread counts
are insensitive to that. **Latency tails and idle-CPU percentages are not**, and
should be read as upper bounds measured under contention rather than as this
machine's best case. The dedicated and shared placements are measured in the
same process, interleaved within seconds of each other and repeated three times
per run, which makes the *comparison* between them far more trustworthy than any
single absolute number here. No baseline was saved from this run; a noisy
desktop is not a reviewed baseline (see
[the runner documentation](../../experiments/README.md)).

## What the fixture does

The new `handoff` family builds one population, then measures it twice inside a
single process:

1. every PTY gets a dedicated blocking reader — the same *shape* as the
   library's arrangement, not the library's code — and the process is sampled
   for memory, threads and CPU;
2. every descriptor is moved, one at a time, onto a bounded set of shared
   readiness workers, and the process is sampled again;
3. every descriptor is moved back to a fresh dedicated reader.

Steps 2 and 3 repeat for three cycles per run. Because both placements hold the
same descriptors, the same counters and the same child processes, the difference
between the two samples isolates reader placement and nothing else.

The fixture lives in `experiments/pty/src/handoff.rs`, with dynamic readiness
registration added to `experiments/pty/src/platform/{linux,macos}.rs`. At 731
nonblank lines it is over the 350-line alarm in `coding_standards.md`, and it is
not in `scripts/gate.py`'s inventory (which does not scan `experiments/pty`), so
the gate did not report it. It holds several separable concerns — interrupt
mechanics, delivery accounting, the shared worker, the placement orchestration
and the measurement case — and splitting it is a reasonable follow-up. It was
left whole here because the recorded source and binary hashes above are what
make this evidence reproducible, and changing the source would invalidate them
without changing what was measured.

Moving a descriptor is the part ADR 0003 calls unresolved platform mechanics, so
the fixture implements them explicitly and counts them:

- A silently blocked reader is interrupted with `SIGURG`, whose handler is
  installed without `SA_RESTART` so `read` returns `EINTR`. `SIGURG`'s default
  disposition is "ignore", so a stray delivery cannot kill anything.
- The signal is retried until the reader acknowledges, because a single signal
  is lost if it lands between the reader's stop check and its next `read`.
- Ownership moves only after the old reader has published every byte it already
  read and has been joined; only then is the descriptor switched to
  non-blocking and registered with a worker.
- The adopting worker immediately drains whatever arrived during the gap.
- A per-session ownership flag, not the readiness registration, decides whether
  a worker may touch a descriptor, so a stale readiness event is rejected.
- Producers write a continuous 256-byte ramp, so the expected value of every
  absolute byte offset is known and loss, duplication or reordering across a
  transition is detected rather than assumed away.
- One echo-probe PTY migrates with the population, so its round trip always
  reports the latency of the placement the population is currently in.

All three mechanics turned out to be necessary, not theoretical. Across the 50
handoff fixture runs — 37,260 transitions in total, 24.4 GB delivered:

| Counter | Total | Runs affected |
| --- | ---: | ---: |
| Transitions measured (each direction) | 18,630 | 50 of 50 |
| Interrupt signals sent | 18,710 | 50 of 50 |
| Repeated interrupts needed (lost wakeup window) | 80 | 42 of 50 |
| Bytes an adopting worker found already waiting | 240,640 | 14 of 50 |
| Readiness events rejected by the ownership flag | 20 | 8 of 50 |
| Bytes lost, duplicated or reordered | **0** | 0 of 50 |

A single unretried signal would have hung 42 of 50 runs. Without the immediate
drain on adoption, 240 KB of already-readable output would have sat unowned.
Without the ownership flag, 20 readiness events would have reached a worker that
no longer owned the descriptor.

## 1. What a dedicated reader costs per idle session

Measured as the difference between the two placements in the same process, with
identical descriptors open. Values are medians of five runs.

| Population | Per reader, RSS | Charged footprint | Live Rust heap | Virtual reservation |
| ---: | ---: | ---: | ---: | ---: |
| 16 + probe | 14.12 KiB | 15.06 KiB | 1,115 B | 256 KiB |
| 64 + probe | 15.51 KiB | 15.75 KiB | 1,238 B | 256 KiB |
| 128 + probe | 15.75 KiB | 15.88 KiB | 1,269 B | 256 KiB |
| 256 + probe | 15.81 KiB | 15.94 KiB | 1,282 B | 256 KiB |

**About 15.8 KiB of resident memory per live PTY, and 256 KiB of virtual
reservation.** The resident figure is one 16 KiB page: these threads request
64 KiB stacks and touch a single page of it. The heap term is the 1 KiB read
buffer plus thread bookkeeping, and it is part of RSS rather than additional to
it. macOS rounds the thread's virtual stack reservation to 256 KiB regardless of
the 64 KiB request, which is why reserved and resident are reported separately,
as [ADR 0002](../adr/0002-performance-and-stability.md) requires.

The fixed-placement idle family agrees from a separate direction. Added owner
RSS over each fixture's own baseline, medians of five runs with ranges:

| Idle PTYs | Dedicated, added RSS MiB | Four shared, added RSS MiB | Dedicated threads |
| ---: | ---: | ---: | ---: |
| 1 | 0.141 [0.125–0.141] | 0.141 [0.125–0.141] | 1 |
| 16 | 0.406 [0.391–0.438] | 0.188 [0.188–0.203] | 16 |
| 64 | 1.359 [1.328–1.375] | 0.219 [0.203–0.250] | 64 |
| 128 | 2.469 [2.453–2.500] | 0.250 [0.219–0.250] | 128 |
| 256 | 4.750 [4.750–4.766] | 0.281 [0.250–0.281] | 256 |

The marginal cost between 16 and 256 idle PTYs is 18.5 KiB per session for
dedicated readers and 0.4 KiB per session for four shared workers.

For scale: ADR 0002 budgets 4 KiB of control state for an idle session and
requires reader stacks and scratch to be counted separately. On this host the
separately counted reader term is about four times the entire control budget,
and 256 idle sessions reserve 64 MiB of virtual address space for reader stacks
alone.

Idle CPU was 0.002–0.007 % of one core for **both** placements at every
population, with per-run ranges that overlap completely. At this resolution, on
this host, idle CPU does not distinguish the two arrangements; a blocked `read`
and a blocked readiness wait both cost nothing while nothing happens. That is a
null result, not a win for either side.

### Cross-check against the real library

The fixture is not the runtime, so the same question was asked of the runtime
itself, using the existing release-load harness rather than new code:

```sh
cargo build --release --locked --example release_load
python3 scripts/release/load.py --case resources-raw-1 --case resources-raw-32 \
  --case resources-raw-128 --repeats 5 --output work/library-idle
```

Five 60-second trials per case, idle raw sessions, no producers, no observers.
All fifteen trials passed.

| Idle raw sessions | Owner RSS MiB | Threads | Live readers | Reader scratch | Rust requested live | Owner idle CPU |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 2.781 [2.766–2.797] | 8 | 1 | 4 KiB | 821,498 B | 0.066 % |
| 32 | 3.891 [3.875–3.906] | 39 | 32 | 128 KiB | 1,061,909 B | 0.116 % |
| 128 | 6.219 [6.188–6.234] | 135 | 128 | 512 KiB | 1,810,927 B | 0.247 % |

The marginal cost of an idle raw session in the library is **24.8 KiB of owner
RSS** (32 → 128; 27.7 KiB over 1 → 128), **exactly one thread**, 4 KiB of reader
scratch and about 7.8 KiB of requested Rust heap — of which 3.7 KiB is not
reader scratch, i.e. inside ADR 0002's 4 KiB control-state budget. The fixture's
15.8 KiB stack page plus the library's 4 KiB scratch accounts for roughly 20 KiB
of that 24.8 KiB; the rest is session control state and allocator overhead.

The idle CPU line matters more than the memory line for the handoff question.
The runtime spends **0.247 % of one core on 128 idle raw sessions** — comfortably
inside ADR 0002's 1 % target — and the fixture shows both reader placements
idling at 0.002–0.007 %. So essentially none of that 0.247 % is being spent by
readers, and moving readers cannot reclaim it.

## 2. What a shared arrangement costs for the same population

| Population (idle) | Shared workers | Added RSS MiB | Added footprint MiB | Threads |
| ---: | ---: | ---: | ---: | ---: |
| 16 + probe | 4 | 0.328 [0.312–0.328] | 0.188 [0.188–0.203] | 4 |
| 64 + probe | 4 | 0.500 [0.469–0.516] | 0.375 [0.328–0.375] | 4 |
| 128 + probe | 1 | 0.531 [0.516–0.531] | 0.391 [0.375–0.391] | 1 |
| 128 + probe | 2 | 0.578 [0.531–0.578] | 0.438 [0.391–0.438] | 2 |
| 128 + probe | 4 | 0.625 [0.609–0.625] | 0.484 [0.484–0.500] | 4 |
| 256 + probe | 4 | 0.891 [0.875–0.906] | 0.766 [0.750–0.781] | 4 |

The shared column still grows with population, by about 2.4 KiB per session
between 16 and 256 sessions, but that residue is per-PTY bookkeeping and kernel
descriptors, not reader placement; the fixed-placement idle family, whose
baseline is taken before the PTYs exist, shows 0.4 KiB per session. Worker count
itself is nearly free: going from one shared worker to four costs 0.094 MiB at
128 sessions.

In thread terms the difference is the whole point: 4 threads instead of 257.

## 3. What the handoff itself costs

Per-transition wall time, from the decision to move until the new owner is
installed and has checked for pending data. Medians of five runs' per-run
percentiles, ranges over runs. Quiet sessions, idle population:

| Population | Workers | To shared p50 | To shared p99 | To dedicated p50 | To dedicated p99 |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 16 | 4 | 64 µs | 192 [145–234] µs | 47 µs | 145 [129–195] µs |
| 64 | 4 | 60 µs | 227 [153–312] µs | 46 µs | 146 [113–180] µs |
| 128 | 1 | 49 µs | 134 [118–140] µs | 46 µs | 130 [123–188] µs |
| 128 | 2 | 54 µs | 128 [119–155] µs | 42 µs | 122 [107–219] µs |
| 128 | 4 | 48 µs | 114 [99–159] µs | 38 µs | 115 [77–133] µs |
| 256 | 4 | 46 µs | 108 [100–123] µs | 37 µs | 103 [88–129] µs |

**On a quiet machine a transition costs tens of microseconds and does not grow
with population.** Parking (dedicated → shared) is the more expensive direction
at the median because it includes interrupting and joining a thread; waking is
cheaper at the median because thread creation is fast when the machine is idle.

Under load that reverses, sharply. The same 128-session population with 16
producers, varying the offered rate per producer:

| Offered per producer | To shared p50 / p99 | To dedicated p50 / p99 |
| ---: | ---: | ---: |
| 256 KiB/s | 49 / 103 µs | 42 / 109 µs |
| 1 MiB/s | 52 / 188 µs | 43 / 223 µs |
| 4 MiB/s | 43 / 344 µs | 29 / 268 µs |
| 16 MiB/s (saturating) | 143 / 327 µs | 128 µs / **8,854 [1,070–14,224] µs** |

For the actively producing sessions in the saturating case the tail is worse
still: to-dedicated p99 of **10,967 µs**, ranging from 745 µs to 21,178 µs across
the five runs. Restoring a dedicated reader means creating a thread, and thread
creation on a machine whose cores are already saturated is a millisecond-scale
operation with a very long tail. That is the single most important number in
this report: **the wake direction is cheap only when the machine is not busy,
which is exactly when you would not need to wake anything.**

Thread creation rate, for the record: 18,630 reader threads were created and
joined across 50 runs with no thread leak — every run returned to its baseline
thread and descriptor count, which the validator enforces.

## 4. Where shared placement wins, and where it starts losing

Same process, same population, same offered load, both placements measured three
times per run. 128 sessions, 16 producers, four shared workers.

| Offered per producer | Aggregate delivered, dedicated | …shared | Probe p99, dedicated | …shared | Owner CPU %, dedicated | …shared |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 256 KiB/s | 4.00 MiB/s | 4.00 MiB/s | 330 µs | 290 µs | 5.13 % | 4.85 % |
| 1 MiB/s | 15.98 MiB/s | 16.00 MiB/s | 531 µs | 484 µs | 19.78 % | 15.41 % |
| 4 MiB/s | 63.99 MiB/s | 63.99 MiB/s | 458 [243–635] µs | 501 [434–642] µs | 142.6 [68.2–150.3] % | 69.9 [61.6–106.8] % |
| 16 MiB/s (saturating) | 155.7 MiB/s | 135.1 MiB/s | 154 [137–198] µs | 562 [334–1155] µs | 312.4 % | 194.0 % |

Expressed as CPU cost per delivered MiB: 12.8 vs 12.1 ms/MiB at 4 MiB/s
aggregate, 12.4 vs 9.6 at 16 MiB/s, **22.3 vs 10.9 at 64 MiB/s**, and 20.1 vs
14.4 at saturation. Dedicated readers do a 1 KiB blocking read per wakeup;
shared workers drain up to a 64 KiB fairness budget per readiness event, which is
where the CPU difference comes from.

Reading the four questions together:

- **On memory, shared placement wins at every population above one**, and the
  saving is simply 15.8 KiB × live sessions: 1.98 MiB at 128 sessions, 3.97 MiB
  at 256. There is no crossover to find, because the dedicated term is linear
  and the shared term is flat. What changes with population is whether that
  saving is worth anything: 0.23 MiB at 16 sessions is noise, 3.97 MiB at 256
  sessions is real but is still 0.016 % of this host's RAM.
- **On CPU, shared placement wins from about 64 MiB/s aggregate upward** — 2.0×
  less CPU per delivered MiB at 64 MiB/s and 1.4× at saturation — and is level
  or slightly ahead below that.
- **On latency, shared placement is level up to about 64 MiB/s aggregate and
  loses above it**: at saturation its probe p99 is 3.6× worse (562 vs 154 µs)
  and it delivers 13 % less throughput. This reproduces, in one process, the
  direction of Experiment 0003's saturated 128-producer result, where dedicated
  readers had the lower probe p99 on both platforms.
- **The transition is cheap when idle and expensive when loaded.** Parking a
  quiet session costs about 50 µs. Waking one back onto a dedicated reader while
  the machine is saturated costs a p99 of 9–11 ms.

So the honest shape of the tradeoff is not "shared wins above N sessions". It is:
*shared placement buys about 15.8 KiB and one thread per idle session, and
roughly half the CPU per MiB at high aggregate rates, at the price of a worse
saturated latency tail and a wake path whose cost is worst precisely when the
system is under pressure.*

## What this does not establish

- **Nothing about the library.** These are standalone transport fixtures: raw
  PTYs, no Ghostty parsing, no session API, no input admission, no control path,
  no projection, no checkpointing. Runtime control latency, cancel/resize
  responsiveness and parser backlog are entirely absent.
- **Nothing about Linux.** Only macOS arm64 was run. The epoll code paths for
  dynamic registration compile but were not executed. Linux's 4 KiB pages alone
  would change the per-reader resident figure, and Experiment 0003 already shows
  the two platforms ranking reader strategies differently. One platform's result
  does not establish the other's.
- **No policy was measured.** The fixture performs a forced, serialized
  migration of an entire population on command. A real policy has thresholds,
  hysteresis and partial populations; flapping, admission interaction and
  thread-churn under an oscillating workload are unmeasured. ADR 0003's
  requirement to "use hysteresis to avoid thread churn" has no evidence here.
- **No lifecycle races during a transfer.** ADR 0003's reader-handoff row also
  asks for concurrent input, cancel and exit during the transfer. This fixture
  migrates while output flows and proves byte integrity, but it never races
  cancellation, child exit, or a resize against a transition. That gap is the
  main reason this spike is not sufficient to authorize an implementation.
- **The library cross-check is narrow.** Raw sessions only — no projection, no
  terminal model — idle only, and capped at 128 sessions by the release matrix's
  own bound. It measures what an idle session costs today; it does not measure
  what handoff would do to it, because the library has no handoff to measure.
- **No 500-session population.** This host's system-wide PTY limit is 511 and is
  shared with everything else running; 256 sessions plus a probe was the largest
  population taken, and no user limit was changed to get there.
- **Idle CPU is not resolved.** Both placements measured 0.002–0.007 % of a
  core with overlapping ranges, on a host with roughly one core of competing
  load. This says the difference is small; it does not measure it.
- **The two placements do not have matched stacks.** Dedicated readers request
  64 KiB stacks; the fixture's shared workers use the default, which reserves
  2,240 KiB of virtual address space each. That inflates the shared side's
  virtual column and does not affect its resident column meaningfully, but it
  means the virtual reservations are not an apples-to-apples comparison.
- **Kernel and child costs are excluded**, as in Experiment 0003. Reported
  memory is owner-process only: no kernel PTY buffers, no child processes, no
  terminal models, no replay.
- **Latency numbers carry desktop contention.** See the environment note above.
  The wide `142.6 [68.2–150.3] %` CPU range in the 4 MiB/s row is the clearest
  symptom; treat single absolute latency figures from this run as indicative.

## What the evidence points toward

Toward **not implementing handoff now**, and toward keeping it deferred with a
recorded price rather than an unknown one.

The saving is real, bounded and small: 15.8 KiB and one thread per idle session,
which at the populations this runtime actually targets is single-digit MiB —
about 64 % of the 24.8 KiB an idle raw session costs the library today, and none
of its idle CPU, because readers are not spending any. The
cost is a new ownership protocol with three failure modes that this run proved
are live — a lost-wakeup window that fired in 42 of 50 runs, stale readiness
events in 8, and pending data at adoption in 14 — plus a wake path whose p99
degrades by two orders of magnitude under exactly the load that would trigger
it. The CPU advantage at high aggregate rates is genuine and the more
interesting finding, but it argues for revisiting the *read batching* of the
dedicated reader (1 KiB blocking reads) at least as strongly as it argues for
moving descriptors between threads.

If the population target ever moves to thousands of idle sessions, the linear
term changes that judgement. At hundreds, it does not.

This is measurement, not a decision. The decision belongs in an ADR revision,
and ADR 0003's remaining reader-handoff requirements — concurrent input, cancel
and exit during a transfer — are not met by this spike.

## Raw evidence

- Handoff profile, macOS: [metadata](data/0004/macos-metadata.json),
  [raw records](data/0004/macos-results.jsonl),
  [validated summary](data/0004/macos-summary.json). Every case, every
  repetition, with the per-run values behind each median.
- Library idle census, macOS: [per-trial rows](data/0004/macos-library-idle.jsonl)
  — one row per trial, fifteen rows. The release harness's full trial logs are
  13 MB of budget events and are deliberately **not** committed; `AGENTS.md`
  forbids checking load dumps into the tree. Re-run the command above to
  regenerate them.

The validator enforces, per handoff record: one reader per live PTY in the
dedicated placement, a thread ceiling never exceeded at the sampled placements
(the fixture samples the steady placements, not every instant of a migration,
so a transient spike between two transitions would not be caught), zero
disordered bytes, a
delivered-byte checksum matching the producer ramp, complete migration samples in
both directions, a reader-thread-creation count matching the measured wake
migrations, and both placements probed. Its unit tests include deliberate
regressions for each of those. Descriptor and thread counts must return to the
fixture's own baseline or the run fails.
