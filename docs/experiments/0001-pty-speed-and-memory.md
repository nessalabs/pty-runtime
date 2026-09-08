# Experiment 0001: PTY reader speed and memory

## Design decision

This experiment informs [ADR 0001](../adr/0001-pty-runtime.md) and
[ADR 0002](../adr/0002-performance-and-stability.md). Compare speed and memory
together when selecting the reader model.

The original shared-reader recommendation is superseded by the accepted
[Experiment 0003 decision](0003-cross-platform-concurrent-workloads.md): use
dedicated readers as the initial default for active and quiet sessions, keeping
shared strategies as comparators. The target includes hundreds of simultaneously
active sessions as well as idle and mixed populations.

The historical measurements below remain unchanged. Dedicated readers were 4.5%
faster in this single-feeder fixture; shared I/O used about 25 times less added
charged memory and 5.7% less CPU per MiB. Its unloaded handshake p99 does not
measure latency under independent concurrent output. Experiment 0003 supplies
that evidence and the accepted latency/memory tradeoff. Automatic reader
migration remains deferred pending separate correctness and performance proof.

Parking should reclaim optional memory while keeping reads and exit monitoring
armed. It must not mean stopping PTY draining. Target a few KiB of idle control
state, with explicit separate budgets for history, terminal models, and work in
flight. Neither kernel PTYs nor live child processes have zero per-session cost.

## Matched speed-and-memory result

Measure both dimensions in each run using 1 KiB read buffers. Dedicated readers
request 64 KiB stacks; the shared reader uses one worker and one buffer. Each
condition ran three times in a fresh process, with reader order reversed in the
second repetition. The 16-PTY cases transfer 128 MiB total; the 128-PTY cases
transfer 256 MiB total. Memory is sampled at the end of the transfer and reported
as the increase over the same process before creating the readers.

| PTYs | Reader | Aggregate throughput | Transfer time | CPU ms/MiB | Sequential handshake p99 | Added live heap | Added charged memory | Threads |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 16 | Dedicated blocking | 235.2 MiB/s | 0.544 s | 5.35 | 8.83 µs | 26.8 KiB | 352 KiB | 17 |
| 16 | Shared kqueue | 226.9 MiB/s | 0.564 s | 5.01 | 9.50 µs | 9.2 KiB | 48 KiB | 2 |
| 128 | Dedicated blocking | 236.0 MiB/s | 1.085 s | 5.32 | 8.33 µs | 187.6 KiB | 2,368 KiB | 129 |
| 128 | Shared kqueue | 225.9 MiB/s | 1.133 s | 5.02 | 9.00 µs | 36.4 KiB | 96 KiB | 2 |

Values are medians of three runs, including the median of each run's p99.
Handshake measurements follow the bulk transfer and include feeder/read/condition
variable notification; they do not measure responsiveness during flooding.
End-of-transfer memory is a sample, not a sampled peak.

All individual matched runs:

| PTYs | Reader | Aggregate throughput in runs 1 / 2 / 3, MiB/s | Sequential handshake p99 in runs 1 / 2 / 3, µs |
| ---: | --- | --- | --- |
| 16 | Dedicated blocking | 228.67 / 237.73 / 235.17 | 8.83 / 8.38 / 9.00 |
| 16 | Shared kqueue | 220.96 / 229.02 / 226.93 | 8.75 / 10.13 / 9.50 |
| 128 | Dedicated blocking | 229.58 / 237.85 / 236.03 | 8.33 / 7.25 / 8.50 |
| 128 | Shared kqueue | 225.14 / 225.88 / 227.80 | 9.79 / 9.00 / 8.88 |

### Why aggregate throughput is similar at 16 and 128 PTYs

Throughput is **total bytes across all PTYs divided by elapsed time**. It is not
the rate achieved by each PTY. There is one feeder thread in both cases. It
performs blocking writes of 4 KiB to each child endpoint in turn, completing the write to
one before starting the next. The shared-reader case also retains one reader
worker at either PTY count. Increasing the descriptor count does not add
independent producers or reader workers.

| Shared reader | Total bytes | Elapsed time | Aggregate rate | Aggregate rate divided by PTY count |
| --- | ---: | ---: | ---: | ---: |
| 16 PTYs | 128 MiB | 0.564 s | 226.9 MiB/s | 14.18 MiB/s |
| 128 PTYs | 256 MiB | 1.133 s | 225.9 MiB/s | 1.76 MiB/s |

These are the matched medians. The last column is an arithmetic average, not a
measurement of per-PTY latency or fairness. The 128-PTY run transfers twice the
total bytes in approximately twice the time, spread over eight times as many
descriptors. The feeder can block on the current child endpoint instead of keeping all
PTYs independently supplied. Both reader models therefore exercise the same
serialized producer path. The similar rates are consistent with a common
feeder/PTY/read pipeline limit; this fixture did not isolate which component
sets that limit or demonstrate saturation of the reader implementation.

The p99 measurements are also serialized: after all bulk bytes are received,
write one byte, wait for its reader notification, then move to the next PTY.
Only one handshake is outstanding globally. Similar p99 values do not establish
latency with 16 or 128 simultaneously active producers.

Use these results for the measured allocation tradeoff and single-feeder
transport comparison. To qualify concurrent capacity, run independent child
producers, sweep offered rate per active PTY, and report aggregate throughput,
per-PTY progress, producer blocking, and input/control latency during output.
The 4.5% speed difference here is not a general limit on the benefit of dedicated
readers under that different workload.

The largest matched-series handshake was 64.04 µs. A same-batch 16 KiB-buffer
control at 16 PTYs produced medians of 238.6 MiB/s blocking and 229.7 MiB/s shared.
Thus the much higher absolute rates than the earlier exploratory series below
cannot be attributed to the smaller buffer. The desktop was not controlled
between batches, and the source gained additional measurement fields. Compare
reader models within a batch; do not claim a fourfold optimization across batches.

The raw records include CPU time, elapsed time, heap, RSS, charged footprint,
virtual reservations, thread counts, transferred byte totals, and latency
summaries. Byte counts and checksums are asserted during each transfer.
[Recorded measurements](data/0001-results.jsonl).

## What the OS does

Output follows `child write → child endpoint terminal processing → kernel output queue →
host endpoint read → backend processing → observer`. Input follows the reverse PTY
direction through the child endpoint's input processing. Canonical mode, echo, signal
characters, and output translations are terminal behavior, not transport
optimizations to disable indiscriminately. These experiments set raw mode to
isolate the transport. [PTY interface](https://man7.org/linux/man-pages/man7/pty.7.html),
[termios behavior](https://man7.org/linux/man-pages/man3/termios.3.html).

The inspected Apple driver sleeps a blocking host endpoint read when no output is
available, or returns EWOULDBLOCK for nonblocking I/O. The child endpoint writer also
sleeps, or returns EWOULDBLOCK, when its output queue is full. Each terminal has
its own allocated state and queues. Thus idle CPU and idle memory are separate
problems, and stopping reads eventually stops progress on output-producing
workloads. [Host endpoint read implementation](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/bsd/kern/tty_dev.c#L622),
[write backpressure and terminal allocation](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/bsd/kern/tty.c).

A shared kqueue worker can sleep until one of many registered descriptors is
ready. Readiness events can coalesce; they are not a message-per-write transport.
Handle partial I/O, drain/revisit ready descriptors within a fairness budget, and
honor EOF and errors. A thread per blocking read avoids some readiness dispatch
work but retains a thread and read destination for each outstanding call.
[Apple kqueue documentation](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/kqueue.2.html).

Closing a descriptor from another thread is not a portable way to cancel an
outstanding read: descriptor reuse and OS-specific blocked-I/O behavior matter.
Keep descriptor ownership explicit and design cancellation independently of data
queue capacity. [Close and concurrent I/O](https://man7.org/linux/man-pages/man2/close.2.html).

The published XNU source revision is `f6217f891ac0bb64f3d375211650a4c1ff8ca1ea`.
It explains mechanisms; it is not claimed to be the exact source of the installed
kernel. macOS-specific numerical results below come from executed experiments.

## Method and scope

- Host: Apple Silicon `Mac17,3`, 10 logical CPUs, 24 GiB RAM, macOS 26.6
  (`25G72`), 16 KiB OS pages. Desktop applications remained running, so these
  are exploratory results with scheduling noise.
- Rust 1.98.1, optimized release build; `libc = 0.2.189` and
  `serde_json = 1.0.151`, with Cargo.lock.
- Two small Rust reader prototypes operate real `openpty` endpoint pairs.
  One uses a blocking thread per host endpoint. The other uses one level-triggered
  kqueue worker, one shared buffer, and a finite read budget.
- One feeder in the same process writes 4 KiB chunks with blocking writes,
  visiting the child endpoint descriptors sequentially; producers are not independent.
  There are **no child processes, Ghostty models, Tokio tasks, retained replay,
  or external event sinks** in these measurements.
- Each comparison has three runs in fresh processes, executed sequentially.
  Idle tests warm each PTY, settle for 200 ms, then measure a two-second quiet
  period. Earlier active tests transfer 8 MiB per PTY and then measure 2,000 one-byte
  write/read/notification handshakes. They validate received byte counts and
  checksums; this is not a full session correctness test.
- Measure requested live Rust heap bytes with an allocator wrapper, RSS and
  thread/virtual-memory counts with `proc_pidinfo`, charged physical footprint
  with `proc_pid_rusage`, and CPU time with `getrusage`.

Tables use medians of three runs. Memory deltas are relative to the same fresh
process before creating PTYs/readers. Charged footprint differs from RSS and
virtual reservations; kernel PTY allocations are not isolated by these counters.

## 1. Idle reader memory

With 16 KiB read buffers and default Rust reader-thread stacks:

| PTYs | Reader | Extra live Rust heap | Extra charged footprint | Total threads | Extra virtual reservation |
| ---: | --- | ---: | ---: | ---: | ---: |
| 1 | Blocking per PTY | 16.5 KiB | 64 KiB | 2 | 2.19 MiB |
| 1 | Shared kqueue | 16.6 KiB | 48 KiB | 2 | 2.19 MiB |
| 32 | Blocking per PTY | 525.7 KiB | 1,200 KiB | 33 | 70 MiB |
| 32 | Shared kqueue | 24.1 KiB | 80 KiB | 2 | 2.19 MiB |
| 128 | Blocking per PTY | 2,102.6 KiB | 4,464 KiB | 129 | 280 MiB |
| 128 | Shared kqueue | 47.4 KiB | 112 KiB | 2 | 2.19 MiB |

A deliberately smaller blocking setup requested 64 KiB stacks and used 1 KiB
read buffers. This avoids judging blocking readers only by generous defaults:

| 128 PTYs | Extra live Rust heap | Extra charged footprint | Total threads | Extra virtual reservation |
| --- | ---: | ---: | ---: | ---: |
| Blocking, smaller stacks and 1 KiB buffers | 183.6 KiB | 2,432 KiB | 129 | 32 MiB |
| Shared kqueue, one 1 KiB buffer | 32.4 KiB | 80 KiB | 2 | 2.19 MiB |

Requested stack size is not the entire thread mapping. Small stacks also require
separate safety qualification before running a native parser on them. Rust's
stack configuration is described in [the threading documentation](https://doc.rust-lang.org/std/thread/index.html#stack-size).

All tested readers used less than 0.01% of one CPU core during the short idle
sample. This supports sleeping rather than spinning; the samples do not qualify
the longer idle-CPU release target. Live Rust allocations and reader-thread counts
returned to their starting values after cleanup. OS allocator footprint did not
always return immediately.

**Implication:** shared buffers/workers eliminate large repeated allocations.
Some compact descriptor and bookkeeping cost remains proportional to PTY count.
The full API's proposed 4 KiB idle-control budget is still a target, not an
integrated-backend result.

## 2. Earlier throughput and wake-latency exploration

Both readers used the same 16 KiB buffer size for this comparison. The blocking
case has one buffer per reader; kqueue shares one. The feeder writes 4 KiB chunks
sequentially across the PTYs. Results include feeder, checksum, notification,
and reader work in the same process.

| PTYs | Reader | Median throughput | Throughput range | Median per-run p99 handshake |
| ---: | --- | ---: | ---: | ---: |
| 1 | Blocking per PTY | 74.4 MiB/s | 49.1–100.6 MiB/s | 65.5 µs |
| 1 | Shared kqueue | 44.6 MiB/s | 31.0–56.9 MiB/s | 62.1 µs |
| 16 | Blocking per PTY | 58.9 MiB/s | 53.7–63.7 MiB/s | 38.8 µs |
| 16 | Shared kqueue | 51.0 MiB/s | 49.2–54.2 MiB/s | 53.1 µs |

Dedicated readers were about 16% faster at 16 PTYs by median throughput.
This validates evaluating the blocking-reader hypothesis. The single-PTY range
is wide, and this test does not establish general production superiority.
Handshake latency is measured after throughput, without simultaneous flooding;
it is not a cancellation-under-load or end-to-end application latency result.

**Implication:** this earlier series also favors dedicated readers on raw speed.
Use the matched result above to weigh speed against memory at the same PTY count
and buffer configuration. Do not run a blocking read on a shared async executor
thread.

## 3. Parking: draining and reclamation

### Stopping reads blocks the writer

In all three runs, the raw child endpoint accepted 1,024 bytes before a nonblocking write
returned EAGAIN. A subsequent blocking write stayed blocked during a deliberate
150 ms pause in host endpoint reads and completed after draining resumed.

This is a measurement of this host and raw-mode configuration, not a portable
PTY capacity constant. It is enough to reject “stop reading until a client
returns” as a transparent parking strategy.

### Reclaiming owned buffers can require more than dropping them

With 128 PTYs still registered on kqueue, allocate and touch 256 KiB of synthetic
disposable cache per session: 32 MiB total. Wait two seconds, release that cache,
then send output to confirm that the registered reader still wakes.

| Cache allocation | Charged footprint while holding cache | After idle wait | After release |
| --- | ---: | ---: | ---: |
| Rust Vec / system allocator | 33.14 MiB | 33.14 MiB | 33.17 MiB |
| Owned anonymous mappings, released with munmap | 33.03 MiB | 33.03 MiB | 1.00 MiB |

Dropping the vectors released about 32 MiB of live Rust allocations, while the
charged footprint stayed high during this observation. Explicitly unmapping the
owned pages reclaimed that footprint. RSS varied independently, including drops
before the cache was released; RSS alone would have obscured this distinction.

All six post-release wake checks succeeded. Vector-cache wake samples were
43–66 µs; mapped-cache samples were 44.7 µs, 74.2 µs, and 3.85 ms. Include that
outlier rather than claiming uniformly negligible wake cost.

These are disposable caches, not a real terminal or replay store. The experiment
does not prove that full terminal state can be discarded safely, or that mappings
are automatically the best allocator for small objects. It supports evaluating
releasable pages for large bounded storage while keeping idle metadata compact.

## Plan changes

1. Budget idle control state separately from native terminal, retained data,
   kernel PTY, and child-process memory. Aim for <= 4 KiB of idle control state.
2. Share I/O/control/reaper workers and scratch buffers. Allocate replay on demand
   within a global budget instead of reserving a full buffer per session.
3. Treat 60 seconds of inactivity as an initial configurable cold-state policy.
   Keep readiness and exit observation armed; reclaim optional caches and empty
   pages. Do not stop draining, close the PTY, or suspend the child.
4. Make raw operation and Ghostty projection explicit at session creation. A
   projected session preserves its parser/screen state while cold. Evaluate
   bounded incremental Ghostty history compression before enabling it by default.
   The pinned API documents opportunistic compression and serialized access.
   [Ghostty compression API](https://github.com/ghostty-org/ghostty/blob/82232ecde55405559dec29c5466cb9e39938cb41/include/ghostty/vt/terminal.h#L2223).
5. Measure latency and charged-memory reclamation, including allocator residuals
   and transition races. Keep the implementation decision open until native
   terminal overhead and the complete pipeline are measured.

## Limits and reproduction

No independent-producer saturation test, 500-session run, Linux execution, native terminal-memory test, adaptive
reader migration, actual 60-second cold transition, or long soak was performed.
The 500-session objective needs a suitable host: this Mac reports
`kern.tty.ptmx_max = 511` system-wide, including other applications' PTYs.
No global limits were changed. The executed maximum was 128 PTY pairs.

The Rust experiment harness and raw JSONL results are kept separately from the
package documentation at
`/Users/nessa/Documents/Codex/2026-09-07/pty-runtime-task/work/pty-os-experiments`.
The initial source snapshot is `archive/initial-main.rs`, SHA-256
`7063e791371dc76be74e9ab7dbb3334c7b1849286c1fa76fae79576290b4ff66`.
The matched-series source is `src/main.rs`, SHA-256
`0931781371609307e3f24579452a446ae7e8443c090ea34a756fe240be87c6aa`.
Cargo.lock SHA-256 is
`b458196b6decde20a50880bfbb818a1776957466bcd7693e8471120b66435926`.

Run from that scratch directory:

```sh
cargo build --release --locked
# Repeat each condition three times, sequentially:
./target/release/pty-os-experiments idle blocking 128 2
./target/release/pty-os-experiments idle kqueue 128 2
PTY_READ_BYTES=1024 ./target/release/pty-os-experiments idle small-stack 128 2
PTY_READ_BYTES=1024 ./target/release/pty-os-experiments idle kqueue 128 2
./target/release/pty-os-experiments bench blocking 16 8
./target/release/pty-os-experiments bench kqueue 16 8
PTY_READ_BYTES=1024 ./target/release/pty-os-experiments bench small-stack 16 8
PTY_READ_BYTES=1024 ./target/release/pty-os-experiments bench kqueue 16 8
PTY_READ_BYTES=1024 ./target/release/pty-os-experiments bench small-stack 128 2
PTY_READ_BYTES=1024 ./target/release/pty-os-experiments bench kqueue 128 2
./target/release/pty-os-experiments stall
./target/release/pty-os-experiments cache 128
./target/release/pty-os-experiments mapped-cache 128
```

The additional idle counts were 1 and 32; the additional throughput count was 1.
The 16 KiB-buffer/small-stack idle condition also ran at 128 PTYs. There were
48 initial measurement runs, 12 matched speed-and-memory runs, and six later
16 KiB-buffer control runs; all 66 completed. Retain raw results with each future
comparison; these short exploratory runs do not replace integrated qualification.
