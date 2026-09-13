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

## What would close G1

1. Bring ProjectedOutput p99 within 20 ms for `128-active`, `chunk-64` and
   `chunk-1`, or record an explicit overload outcome identifying the rejected
   operation where the workload is genuinely beyond capacity.
2. Fix the macOS `lsof` census failure and complete the macOS matrix, including
   `dominant`.
3. Run the remaining ADR 0004 items this matrix does not cover: the
   after-all-observers-detach case, split UTF-8/VT sequences and terminal
   queries under load, and an explicit fairness outcome.
