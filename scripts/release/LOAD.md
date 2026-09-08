# Integrated release load qualification

Build the frozen source with `cargo build --release --example release_load
--features event-stream`, retaining the build output and exact source archive.
Run from the repository root:

```
python3 scripts/release/load.py --list
python3 scripts/release/load.py --output /absolute/evidence/load-smoke --smoke
python3 scripts/release/load.py --output /absolute/evidence/load-full
```

The default matrix executes five post-warmup 60-second trials per case. It includes
64 resident / 16 active projected sessions producing 10 MiB/s; attached, detached,
stalled observer, stalled actual event-stream publisher, and dominant producer
modes; unpaced `capacity-projected` and `capacity-raw` cases; 128 independent active producers and 128-session mixed populations; rate,
chunk, observer and grid sweeps; 64 raw idle sessions; and raw/projected physical
resource measurements at 1, 32 and 128 sessions. Each session has a 1 MiB replay cap,
a 1 MiB native history target and an 8 MiB native requested-allocation ceiling.
Idle parking is set to one hour to hold the specified resident population stable.
The root stress harness separately qualifies automatic parking and disk wake.

`--case NAME` may be repeated. `--smoke` reduces counts, rate and durations and is
never release acceptance. `--repeats N` is an explicit override; fewer than five
trials do not fulfill the performance acceptance requirement. No existing output
directory is overwritten. Full 500-session qualification remains host-dependent
and is not included in this bounded 128-session matrix.

Each PTY runs an independent producer. Controlled cases retain explicit rate limits;
capacity cases let active producers write without pacing (`--mode saturation --rate 0`). Its deterministic stream
mixes ASCII, ANSI colors/cursor queries, wide and combining UTF-8; 4093-byte chunks
split both UTF-8 and escape sequences across writes. Control records use a separate
bounded Unix socket. Input probes, same-grid resize operations and one transient
raw cancellation session run while producers continue independently. That dispatch
probe inherits ignored TERM before its control-ready barrier and stays alive through
a 250 ms grace until SIGKILL; this makes direct and verified group TERM dispatch
observable. Its completion is polled while fast observers continue draining. Normal
fast-exit cancellation correctness remains covered by the separate lifecycle tests. The reported
population excludes that explicitly reported transient cancellation probe.

The driver validates every observed byte at its absolute cursor, exact gaps and
retained suffixes, projected processed offsets, and all authoritative query replies.
A fast observer falling behind fails a controlled attached/dominant/stalled-sink trial.
Capacity observers continue polling but may report explicitly accounted eviction gaps. Slow/detached observers must account
for every omitted byte explicitly. A stalled publisher holds exactly one in-flight
sink payload while PTY parsing, replay and controls continue. Logical quota snapshots
must remain within all bounds and return to zero after final wait/forget.

Warmup ends only after producers stop, raw and projected offsets match emitted
bytes, query replies and input acknowledgements settle, and input/control waits have
completed. Internal histograms are then reset at that quiescent boundary. Measurements
include p50/p95/p99/max, successes, failures, unavailable counts and raw buckets for
all eight admission/dispatch/output boundaries. Fixture RTT is reported separately.
Per-producer start skew, emitted bytes and shared-host monotonic start/end timestamps
are retained. Aggregate accepted throughput uses the earliest active producer start
to latest active producer end, excluding owner completion/settling overhead; that
overhead remains separately reported. Fixture RTT includes PTY transport and fixture
scheduling; it has no invented absolute SLA and is not compared with InputDispatch's
internal-boundary target. Missing RTT percentiles are null.

Producers open their actual slave TTY on a separate nonblocking file description,
leaving input blocking behavior intact (one additional reported fixture FD). Every
positive partial write advances the exact byte ledger. `write_blocked_ns` now means
elapsed POLLOUT waits following EAGAIN, including scheduling/wakeup overhead;
`max_backpressure_wait_ns` is the largest such wait. `producer_writes` separately
records total/max syscall elapsed time, attempts are in `producer_done`, and EAGAIN
and partial-write counts are explicit. Syscall elapsed time is not labeled blocking.
Each phase has a real deadline checked between writes and a finite 8 GiB cap per
producer (`--producer-bytes`, allowed 1 byte through 64 GiB). Hitting that cap fails
the trial as censored rather than reporting rate-limited capacity as success.

Capacity is for this integrated fixture, including payload generation, validation,
observers and concurrent control work; it is not an isolated transport ceiling.
Use five full repeats and compare against the same reviewed host/source baseline.
Controlled 10 MiB/s runs demonstrate behavior at that offered load, not maximum
capacity. All replay/native quotas, final raw/projected offsets, query reply checks
and retained-byte/gap validation remain enabled in capacity trials. Latency target results are separate from successful execution of the
correctness/accounting checks; a completed trial can still miss a latency target.
`summary.json` labels `all_trials_passed` as execution/correctness accounting only.
Its separate `target_rollup` records applicable, passed, failed and unmeasured
trials for latency and idle CPU. `all_applicable_passed` is null when no target
applies or required evidence is missing, unless a recorded failure already
establishes false. Per-trial fields identify missing latency boundaries and idle
acceptance duration. Smoke target results do not establish release acceptance.

The Python runner records process-tree RSS/PSS where available, CPU, FDs and threads
by runtime owner, guardian helpers and workload fixture, with census duration and
missing values explicit. Requested Rust heap totals exclude native C/OS allocations
but include fixture objects and the Tokio test driver. They are a conservative input
to control-memory analysis, not an isolated <=4 KiB/session proof. Reader stacks,
scratch, native models, kernel cost, fixed diagnostic storage, helper overhead and
fixture bookkeeping must be accounted separately before claiming that target.
Darwin PSS and portable wakeup counts are unavailable. CPU includes fixture-driver
and sampling overhead. Exact raw and processed-byte accounting does not substitute
for canonical terminal equivalence tests.

Each trial stores source inventory, source/diff and binary hashes, toolchain and host
identity, configuration, all raw records, failures and timeout outcomes. Binary hashes
alone do not bind a dirty source tree to a prior build: retain the frozen source build
log. Compare a reviewed host baseline against repeat distributions; ADR 0002 treats
repeatable >10% throughput or p99 regressions as review failures. Smoke success and
shared CI timings cannot establish a reviewed release baseline.
