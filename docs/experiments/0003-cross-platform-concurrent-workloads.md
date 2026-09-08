# Experiment 0003: Concurrent PTYs and native state on macOS and Linux

Executed 2026-09-08. Both platforms passed 49 cases × five repetitions, or
245 fixture processes each, using the same experiment source. Case order was
reversed on alternate repetitions. These are standalone transport and native
fixtures; the production session runtime remains unimplemented.

## Environments and reproducibility

| Host | CPU / memory | OS / pages | Rust |
| --- | --- | --- | --- |
| macOS arm64 | Apple M5, Mac17,3; 10 logical CPUs; 24 GiB | Darwin 25.6.0; 16 KiB | 1.98.1 |
| Linux x86_64 Box VM | Xeon Skylake; 4 vCPUs; 7.57 GiB exposed RAM | Linux 6.8.0-117, glibc 2.39; 4 KiB | 1.97.1 |

Both used Zig 0.16.0 and the SHA-pinned Ghostty source in
[dependencies.json](../../experiments/dependencies.json). Desktop/VM scheduling
noise and different hardware/toolchains prevent attributing cross-host speed
differences solely to the operating system.

The [runner documentation](../../experiments/README.md) gives commands and gate
semantics. Source manifest SHA-256:
`f0125266ad6ffad5ffe660ffd441473ca7a924a8736570f1a2dc452db48e3fc3`.

## 128 independently active producers

Each producer has its own child process and PTY and writes unthrottled for a
nominal three seconds. A separate echo child/PTY measures input round trips under
load. Readers use 1 KiB buffers and a 64 KiB readiness budget. Completion includes
blocked writes and final drain. Values are medians of five runs; brackets show
minimum–maximum. Latency is the median of each run's probe p99, not a pooled p99.

| Platform | Readers | Throughput MiB/s [range] | Probe p99 ms [range] | Owner CPU ms/MiB |
| --- | --- | ---: | ---: | ---: |
| macos | 1 shared | 217.3 [185.3–236.5] | 8.78 [1.67–31.99] | 3.63 |
| macos | 2 shared | 170.0 [147.9–264.0] | 11.97 [4.15–76.96] | 5.89 |
| macos | 4 shared | 133.8 [124.2–321.6] | 9.98 [4.15–64.93] | 8.68 |
| macos | Dedicated | 154.7 [149.3–160.8] | 0.48 [0.41–7.88] | 16.60 |
| linux | 1 shared | 87.9 [81.8–94.6] | 55.76 [45.75–62.55] | 10.15 |
| linux | 2 shared | 163.7 [144.8–173.4] | 47.55 [42.00–76.58] | 9.10 |
| linux | 4 shared | 198.9 [179.9–206.3] | 45.32 [44.45–64.06] | 8.55 |
| linux | Dedicated | 194.7 [173.8–212.6] | 12.94 [10.07–18.17] | 9.48 |

One shared worker gave the highest measured macOS throughput. Four shared
workers gave the highest measured Linux throughput, close to dedicated readers.
Dedicated readers had lower median probe p99 on both hosts under saturation.
More shared workers did not monotonically improve throughput or latency on macOS.
Do not claim a universal throughput winner. The initial reader choice below
prioritizes latency and accepts the measured memory/CPU tradeoffs.

### Accepted initial default: dedicated readers (2026-09-08)

Use one dedicated reader per live PTY on macOS and Linux, including quiet PTYs.
Bound their number through session admission and keep the strategy behind the
platform process interface. Dynamic movement of idle readers is deferred.

For the 128-active-producer Linux cases above (plus one echo-probe PTY), median
added owner-process memory and median per-run probe p99 were:

| Readers | Probe p99 | Added RSS | Added live Rust heap | Reader threads |
| --- | ---: | ---: | ---: | ---: |
| Dedicated | 12.94 ms | 1.773 MiB | 269.3 KiB | 129 |
| Four shared | 45.32 ms | 0.375 MiB | 111.7 KiB | 4 |

Heap is part of process memory, not an amount to add to RSS. Values are changes
from each fixture baseline, excluding child processes, kernel PTYs, terminal
models, and replay. Dedicated readers cost about 1.4 MiB extra RSS while
delivering similar throughput and lower probe latency. We accept that tradeoff.
On macOS the latency gain comes with lower throughput and higher owner CPU than
one shared worker, as the preceding table shows.

This is an initial implementation decision, not release qualification. Verify
integrated terminal parsing, input/control responsiveness, idle reader costs,
stack sizing, shutdown interruption, and larger session populations before
release. Keep all existing reader configurations in the experiment matrix;
these conclusions do not change the raw evidence or performance baselines.

## Mixed active and idle population

128 producer PTYs remain registered, with 16 independent children each limited
to 0.625 MiB/s, plus the separate echo probe. All reader configurations sustained
about 9.98–9.99 MiB/s. Median per-run probe p99:

| Platform | 1 shared | 2 shared | 4 shared | Dedicated |
| --- | ---: | ---: | ---: | ---: |
| macos | 0.996 ms | 0.325 ms | 0.752 ms | 0.696 ms |
| linux | 2.433 ms | 1.529 ms | 1.645 ms | 1.766 ms |

The qualification profile also covers idle populations of 1, 16, and 128 PTYs,
serial transport at 16 and 128 PTYs, and 16 fully active producers. Byte totals,
checksums, per-producer progress, descriptor cleanup, and joined-reader cleanup
passed. This establishes transport evidence at 128 active producers; larger
populations, longer runs, native parsing in the same pipeline, resize, and
cancellation under load remain release work.

## Native compression and parking

These separate C fixtures fill 128 terminal models with 10,000 varied lines each,
then checkpoint, free, restore usable state, restore complete history, and verify
state. Values below are median added process memory over each fixture's baseline.
macOS uses charged footprint; Linux uses private memory from smaps. They are
separate counters, not equivalent measurements.

| Platform / allocator | Filled MiB | After compression MiB | Parked MiB | Checkpoint files MiB |
| --- | ---: | ---: | ---: | ---: |
| macos / default | 815.41 | 336.70 | 292.95 | 96.64 |
| macos / experimental mapping | 816.50 | 336.66 | 2.14 | 96.64 |
| linux / default | 799.57 | 306.80 | 3.44 | 96.64 |
| linux / experimental mapping | 799.57 | 306.81 | 2.88 | 96.64 |

On macOS the compressed/default allocation path retained about 293 MiB after
parking; the experimental mapping allocator reduced this to 2.14 MiB. Linux's
default path already fell to 3.44 MiB, versus 2.88 MiB with mapping. The macOS
reclamation problem must not become a universal allocator requirement. Mapping
is still a fixture comparison, not a production pool implementation.

Checkpoint files occupy about 96.64 MiB separately from the parked process.
Reported memory excludes PTYs, child processes, replay, encryption, and clients.
The raw evidence also records RSS, Linux PSS, restoration timing, and repetitive
content cases. Thirty-sample codec trials and native continuation/corruption
checks passed on both hosts. This is not complete terminal conformance or
concurrent Rust/Ghostty integration.

## Gates and decision impact

The CLI validates every raw result and requires complete case/repetition coverage.
Nineteen unit tests exercise invalid data, cleanup, timeout handling, incompatible
inputs, and deliberate latency/throughput/memory regressions. Separate candidate
baselines are saved under `experiments/baselines`. Comparing each run to its own
baseline passed (392 macOS metrics, 472 Linux metrics); this verifies gate
self-consistency, not absence of regressions against an independent revision.
A Linux-to-macOS baseline comparison correctly returned exit status 2.

The GitHub Actions workflow is configured for Linux/macOS smoke runs and manual
same-runner revision comparisons. It has not been executed in GitHub Actions.
A candidate baseline from either host is unsuitable for unrelated CI hardware.

Keep worker policy and reclamation inside platform infrastructure. Core budgets
and terminal contracts remain portable, following
[ADR 0005](../adr/0005-domain-boundaries-and-adapters.md). The next production
qualification must include parsing, boundary conversions, bounded backpressure,
process lifecycle, and control latency alongside active/idle sessions.

## Raw evidence

- macOS: [metadata](data/0003/macos-metadata.json), [raw records](data/0003/macos-results.jsonl), [validated summary](data/0003/macos-summary.json).
- Linux: [metadata](data/0003/linux-metadata.json), [raw records](data/0003/linux-results.jsonl), [validated summary](data/0003/linux-summary.json).
