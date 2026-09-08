# Candidate 4 projected load evidence

All ten full-duration trials completed execution and correctness accounting: five unpaced projected-capacity trials and five controlled dominant-producer trials. **Latency acceptance did not pass:** all five capacity trials exceeded the 20 ms projected-output p99 target; capacity trial 4 also exceeded the 100 ms resize p99 target. All five dominant trials passed the measured latency targets. `summary.json` correctly limits `all_trials_passed` to execution and correctness accounting; the full release matrix was not executed.

## Identity and workload

Every trial records source revision `487ef087efef0337a5adbad58e36806ac3f793ef`, empty source diff SHA-256 `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`, and loaded binary SHA-256 `ac3a9014e20c04e3b01ff41e1e4154cf5c180f07cd7c82b4b8bf4591d787833b`. Platform: macOS 26.6 arm64, Apple M5, 10 CPUs, 25,769,803,776 bytes physical memory. The first JSONL record preserves the source-file inventory, binary identity, exact invocation and toolchain. The accompanying [build metadata](../candidate4-build/metadata.json) and [build log](../candidate4-build/command.log) record successful `cargo build --release --examples --features event-stream`, Rust 1.98.1, unchanged source inventory and the same revision. Source and binary hashes identify these records; they are not a substitute for the build record.

Each trial uses 64 projected sessions, 16 active producers, one observer per session, 80×24 terminal dimensions, 4,093-byte producer chunks, 10 s warmup and 60 s measured producer window. The frozen source retains 256 per-session staging slots. These records precede the later fixture staging-slot sweep and reader-gauge changes.

Capacity uses `--mode saturation --rate 0`: active producers write without pacing, with a finite 8 GiB producer budget. Dominant uses `--mode dominant --rate 10485760`: a controlled aggregate 10 MiB/s reference, allocating 90% to one producer and the remainder across the other active producers. Controlled offered-rate results are not a measurement of maximum capacity. Accepted throughput divides accepted measured bytes by the recorded producer window; driver completion time is recorded separately.

The coordinating run reported concurrent frozen candidate-3 resource workloads on this host. These are contention-affected observations, not isolated machine-capacity results. No causality, default-budget change, or performance improvement is established by comparing them with earlier runs.

## Per-trial results

All ten trials emitted `complete`, a true correctness `trial_result`, and 64 final byte-accounting ledgers with zero recorded gap bytes. This checks the fixture's deterministic raw bytes, processed offsets, command/reply paths and cleanup assertions; it does not replace canonical terminal-state reference tests. All five latency boundaries were measured in each trial, with zero operation failures and unavailable measurements. A threshold miss is distinct from an operation failure.

| Trial | Correctness | Accepted MiB/s | Projected-output p99 ms (target ≤20) | Resize p99 ms (target ≤100) | Latency targets |
|---|---|---:|---:|---:|---|
| [capacity-projected-1](capacity-projected-1.jsonl) | passed | 61.895 | 80.900 | 82.000 | failed |
| [capacity-projected-2](capacity-projected-2.jsonl) | passed | 60.570 | 81.100 | 84.800 | failed |
| [capacity-projected-3](capacity-projected-3.jsonl) | passed | 57.043 | 94.400 | 97.200 | failed |
| [capacity-projected-4](capacity-projected-4.jsonl) | passed | 53.677 | 176.533 | 165.482 | failed |
| [capacity-projected-5](capacity-projected-5.jsonl) | passed | 52.766 | 91.200 | 92.800 | failed |
| [dominant-1](dominant-1.jsonl) | passed | 9.998 | 0.200 | 0.400 | passed |
| [dominant-2](dominant-2.jsonl) | passed | 9.997 | 0.200 | 0.400 | passed |
| [dominant-3](dominant-3.jsonl) | passed | 9.997 | 0.200 | 0.300 | passed |
| [dominant-4](dominant-4.jsonl) | passed | 9.998 | 0.200 | 0.300 | passed |
| [dominant-5](dominant-5.jsonl) | passed | 9.998 | 0.200 | 0.300 | passed |

Input dispatch, raw output and cancel dispatch met their recorded thresholds in all ten trials. Fixture round-trip p99 is a separate measurement, not the runtime input-dispatch boundary: capacity trials ranged 5.5–70.8 ms and dominant trials 3.3–3.4 ms. Producer write-backpressure distributions and per-producer evidence remain in the JSONL records; no queue-only latency conclusion should omit them.

## Earlier observed failures

Frozen candidate 3 (`5373d8e6f0f263b8a25d7dc2b22ba3bf841b9bef`, binary `62bc6fbaf91b02e226038075da2da81dc9dd706bec14f9c145da2dc7eaf49296`) has five completed failed projected-capacity attempts in [the preserved candidate-3 records](../candidate3-capacity-resources/) (`capacity-projected-{1..5}.jsonl`). Each records `Error: Projection(Capacity)`, exit code 1 before driver cleanup, no driver kill, and no successful completion. These are admission-error observations, not five failed latency measurements or valid throughput-capacity results. Their last checkpoint is `ready`.

Candidate 2 (`52ad04c3519616e6cedea9ac8707406970a40ed7`, binary `898cfc9cb198947557e8c4076f6e5e1ceb2c49b6370ce079670332f29189a811`) completed dominant trials 1–4 but dominant trial 5 recorded `Error: Projection(Capacity)` and exit 1 before driver cleanup, last checkpoint `measurement_start`, in `work/release-candidate-2/docs/verification/release/load-observers-candidate2/dominant-5.jsonl`. Candidate 4 has no such admission exit in its ten trials. The earlier failures remain evidence; these bounded new successes do not establish that every possible admission/concurrency failure is eliminated. Candidate 2 also used the earlier driver-completion throughput denominator, so its throughput numbers are not directly comparable with the producer-window measure here.

The behavioral regression tests and independent control-admission reviews carry the mechanism-specific proof. This report supplies workload observations only. It neither resolves the deferred native crash investigation nor claims release PASS, 100% coverage, 500-session acceptance or the 12-hour soak.

## Artifact integrity

SHA-256 values below identify the completed records read for this report. No source, raw result or workload was changed or executed by this report author.

| Artifact | SHA-256 |
|---|---|
| capacity-projected-1.jsonl | `08baba0d4a1ae2751932fb6ff578f9ed4662590c150335d9a86f2b60423d79c3` |
| capacity-projected-2.jsonl | `1ff75ffd968eb4132a585f9d30ccec1905d3164533df77c114315e014974bb3f` |
| capacity-projected-3.jsonl | `dbfe2234c7f8878c3e320b217824cb5f1c5f9097b7670cbd926883a2c4f70464` |
| capacity-projected-4.jsonl | `eacfd5676a549388ab64db05905f9698c958b2fbe7c9d7e321c56387941f2e42` |
| capacity-projected-5.jsonl | `5770f397fefbeb4df501161a443a26c1f566cffc03c6b4c834ddb8a37bf245bd` |
| dominant-1.jsonl | `aa05b35395c3740a595657ccef7d65714d4ae4e5d45f6bec27db03e24cb26c63` |
| dominant-2.jsonl | `80dd1e3fd394f9d7d3b8e25ed4ed71cb4156d221ff9ec369f550de43145a4916` |
| dominant-3.jsonl | `be819f2d3e311ae234f939d504fd70f06770f32f633334d7f395f962980079f0` |
| dominant-4.jsonl | `bc8dd95c4612db7dedee3b5d306025a59101de5ec70d815bd1cf7f4f09fb11eb` |
| dominant-5.jsonl | `d977f01e85642c9f1a0aac7eb5de5ce99b75a04e7337a49e741e54a323136722` |
| summary.json | `5b385e7e635dd44299e92f0221b29b8bbdc9d6b60a78ff8dba8ada4fecc28197` |
| candidate3/capacity-projected-1.jsonl | `1a12c70370de2b8bab544c51025016b528bf6c724c44de2c8160edf9d2048c11` |
| candidate3/capacity-projected-2.jsonl | `e6731d019e01049baa5730dd767156d6ec8d5beffa0567619733561ff8ff55d2` |
| candidate3/capacity-projected-3.jsonl | `315989eb6d5c42ebb5b43feca65487c4a2057b8e66cb1ab7febebb81e724947a` |
| candidate3/capacity-projected-4.jsonl | `b0be7f04b60ec15d6e361309f633fdb11c949fdaa877ed08159592634228a3a3` |
| candidate3/capacity-projected-5.jsonl | `24aa3d7c778113e74b03019f310b52906bf2086ad0c51abacef2361a1cf9bfd3` |
| candidate2/dominant-5.jsonl | `6724857507fd2bf13a8236d3f5dbae816c64afd75debee379c9e70422938aa28` |
