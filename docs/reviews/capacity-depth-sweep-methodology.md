# Independent methodology review: capacity depth sweep wrapper

Reviewed `work/run-capacity-depth-sweep.py`, SHA-256 **95531025546503889ff74b608cc9c5e36a7be23fef62609937c29c290c5b8a5c**, together with the ordinary release driver, matrix and staging-slot CLI contract. Static review plus pure configuration evaluation only: no trials, builds, source changes or native investigation. The reviewer authored the staging-slot fixture feature; this verdict covers the separately authored wrapper and execution methodology, not an independent implementation verdict on that feature.

**Accept the wrapper for the proposed sweep, subject to using the new frozen checkout and matching binary.** No smoke, clone, recursion or parameter-forwarding bug was found in the proposed invocation:

```
--case capacity-projected --case capacity-depth-128 --case capacity-depth-64 --case capacity-depth-32
```

The wrapper saves the original matrix function before replacing it and clones the normal `capacity-projected` case. Pure evaluation confirms all four cases have 64 resident/16 active projected producers, saturation mode, rate 0 (unpaced), chunk 4093, one observer, 80×24 grid, 60-second measurement and 10-second warmup. Only the three added cases supply `staging-slots` 128/64/32; the ordinary capacity case retains the fixture's default 256. The hyphenated dictionary key is correct because the unchanged driver directly constructs `--staging-slots VALUE`.

Without `--smoke` or `--repeats`, the ordinary driver chooses five full repetitions: **20 post-warmup 60-second trials**, retaining normal metadata, census, timeouts, correctness checks, target failures and output files. The wrapper does not bypass target reporting or change benchmark durations. Explicit smoke/repeat overrides remain accepted by the normal CLI, so omit them for this qualifying execution. A fresh `--output` is required. The four selected cases are a depth sweep, not the entire release matrix; `full_matrix_executed=false` is expected and truthful.

## Frozen-source precondition

The wrapper uses **Path.cwd()**, not a hard-coded frozen directory. Launch it with cwd set to the new frozen checkout containing the staging-slot implementation, and use its matching release binary via `--binary` or the correct frozen default `target/release/examples/release_load`. Candidate 4 predates the flag: running that old binary would ignore the extra argument and invalidate the sweep. Do not infer freezing from the wrapper docstring alone.

Retain this wrapper hash alongside the frozen driver/source/build and binary identity because a wrapper outside the checkout may not appear in the driver's git-based source inventory. In every trial, verify the structured `start.projection_staging_slots_per_session` is **256/128/64/32** as named, and that configuration and producer mode are projected/unpaced. Absence or mismatch invalidates the case comparison; the numeric CLI argument alone is insufficient evidence that an older binary applied it.

## Required comparison

Compare all five-trial distributions for accepted bytes per actual producer window, per-producer throughput/fairness, ProjectedOutput p50/p95/p99/max and counts, raw/input/control boundaries, fixture RTT p50/p95/p99/max and samples, producer `write_blocked_ns`/maximum POLLOUT wait plus EAGAIN/write statistics, reader/resource gauges and exact final cleanup. Keep all failed/unmeasured target comparisons and censored/failed executions visible; a correctness pass is not a latency pass.

A shallower parser queue can move waiting upstream into the PTY/producer before the host-read timestamp. Therefore **lower parser latency alone cannot establish an end-to-end improvement**: assess accepted throughput, producer write blocking and fixture RTT together. Do not substitute sum of syscall elapsed time for blocking. Preserve the default-256 results and the controlled 10 MiB/s latency-reference qualification; this sweep does not relax either requirement.

The requested order runs five trials per depth in sequential blocks. Record competing load and inspect chronology for host/thermal drift before attributing small differences to depth. If a product-default decision depends on close results, confirm it with a separate comparable repeat/order check. No product default change or performance acceptance is established by this wrapper review.
