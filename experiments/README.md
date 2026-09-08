# Repeatable PTY and native terminal experiments

These are standalone experiment fixtures for macOS and Linux on arm64/x86_64.
They are separate from the proposed Rust session library. Linux uses epoll;
macOS uses kqueue. Both support one, two, or four shared readiness workers and
dedicated readers. Native fixtures exercise the pinned, real Ghostty C API.

Dedicated readers are the accepted initial runtime default; shared strategies
remain in both profiles to compare the tradeoff. This decision changes no fixture,
raw measurement, or baseline. See [Experiment 0003](../docs/experiments/0003-cross-platform-concurrent-workloads.md).

The target includes hundreds of active sessions and mixed active/idle workloads.
The qualification profile runs 128 independent producers, a mixed population,
and idle populations. Its transport tests do not yet include native parsing in
the same I/O pipeline or qualify the complete session API.

## Run

Prerequisites: Python 3.9+, Cargo/Rust, and a C compiler. Native builds download
Zig 0.16.0 and the exact Ghostty revision in `dependencies.json`, verify archive
SHA-256 values, and cache builds under `work/experiment-cache`. No system-wide
PTY, descriptor, or process limits are changed. Results must use a new directory.

From the package root:

```sh
python3 experiments/run.py run --profile smoke --output work/smoke
python3 experiments/run.py run --profile qualification --output work/qualification
```

Use `--suite pty` or `--suite native` to run one family. `--cache PATH` changes
the build cache location; `--jobs N` bounds build parallelism; `--timeout SECONDS`
bounds each fixture. A timed-out fixture's process group is terminated and its
failure is retained. `--repetitions N` is useful during investigation; fewer
than five repetitions cannot become a performance baseline.

The smoke profile validates operation and structured evidence quickly. The
qualification profile has five repetitions with reversed case order on alternate
passes. These short experiments are not the full runtime's longer release
workloads, integration tests, or soak qualification.

## Cases and meaning

| Family | What runs | What the result means |
| --- | --- | --- |
| Idle PTYs | Raw endpoint pairs, warm-up, shared or dedicated readers, no child processes | Owner allocation, worker/descriptor cleanup, and idle CPU |
| Serial feeder | One producer writes 4 KiB chunks across endpoint pairs | Aggregate transport through a serialized feeder; retained for comparison with Experiment 0001 |
| Concurrent producers | Separate child processes, coordinated start, one output-producing child per active PTY | Aggregate throughput and per-producer progress with independently active endpoints |
| Under-load round trip | One extra PTY with an echo child while producers run | Host input → child read/write → host read/notification latency during output; not runtime cancellation or resize latency |
| Native lifecycle | Filled models, optional resident compression, checkpoint files, model release, READY, full restoration, cleanup | Native CPU, storage bytes, state preservation, and platform-specific memory reclamation |
| Native codec | Five warm-ups and 30 in-memory samples per process | Encode, READY, and full-decode timing, separate from disk wake latency |
| Native correctness | UTF-8/VT continuation, binary round trips, repeated restore, resize, replies, corruption | Deterministic C API checks; not full conformance, fuzzing, or concurrent Rust integration |

Concurrent cases add one probe PTY and echo child to the stated producer PTY
population. Inactive PTYs remain registered. The producer byte counts and time
spent in write calls are saved individually. A rate of zero means unthrottled
output; mixed cases specify a finite rate per producer. Timed producers can
finish a blocked write after their intended deadline; aggregate elapsed time
includes completion and final drain. Child CPU is recorded separately; reported
owner memory excludes child-process memory and kernel PTY allocations.

Readers use 1 KiB buffers and a 64 KiB fairness budget per readiness event.
Dedicated readers request 64 KiB stacks. Shared worker count is bounded and fixed
within each fixture; there is no dynamic reader handoff. Byte counts/checksums
are asserted. Cleanup permits up to one second for OS thread-count reporting to
settle after joined readers, records that delay, and still fails if counts do
not return to baseline.

The native mapping allocator is an experimental comparison with a 4 KiB
allocation threshold and actual OS page rounding. It is not the production pool
design. Native checkpoint files contain only synthetic test data and are not
encrypted. Production encryption and the replaceable checkpoint store remain
planned in [ADR 0003](../docs/adr/0003-session-parking-and-state-transfer.md).

## Evidence and gates

Each result directory contains:

- `metadata.json`: platform, CPU, page size, limits, toolchains, dependency pins,
  workload, binary/source hashes, and expected repetitions.
- `results.jsonl`: every completed or failed fixture with its raw records.
- `summary.json`: validated per-case medians and ranges, written only when all
  expected cases and repetitions pass.
- `logs/`: stdout and stderr for each fixture; `failure.json` when a run fails.
- `gate.json`: regression details when a baseline comparison is requested.

The validator rejects missing or duplicate cases, missing repetitions, nonfinite
metrics, wrong byte totals, failed restoration, and unreleased descriptors or
reader threads. It revalidates raw results instead of trusting a saved summary.

Create a baseline explicitly after reviewing a complete run:

```sh
python3 experiments/run.py baseline --results work/qualification --output work/reviewed-baseline.json
python3 experiments/run.py run --profile qualification --output work/candidate --baseline work/reviewed-baseline.json
```

Or compare an existing result:

```sh
python3 experiments/run.py compare --results work/candidate --baseline work/reviewed-baseline.json --output work/comparison.json
```

Performance comparison requires at least five repetitions and an identical
platform/machine/toolchain fingerprint and workload. Linux PSS/private bytes and
macOS charged footprint remain separate metrics; unavailable metrics are null.
RSS is recorded on both systems. No Apple-to-Linux performance baseline is used.

The gate flags median throughput losses or CPU/latency/memory increases greater
than 10%, subject to small absolute allowances: 1 µs for latency, 1 KiB for live
Rust heap, 64 KiB for OS memory counters, and 0.05 percentage points for idle CPU.
Reported min/max values expose run variation; the gate does not establish
statistical significance. Investigate failures on noisy/shared machines and
repeat on stable hardware before accepting a baseline change. Baselines are
never automatically updated to make a regression pass.

Exit status is zero for successful validation/comparison, one for a failed run
or performance gate, and two for invalid comparison input or incompatible data.
Build failures and unrun cases never count as passing evidence.

## CI

The supplied [workflow](../.github/workflows/experiments.yml) runs smoke checks on
Linux and macOS for pushes and pull requests and retains logs on failure.
Its configured runners are not evidence of executed platform coverage.

For a performance gate, manually dispatch the workflow with `qualification` and
a `baseline_ref` commit or tag that contains the same experiment protocol. The
workflow measures that revision and the current revision sequentially on the
same runner, then compares them. A qualification dispatch without a comparison
revision gathers candidate evidence only. Changing the workload requires a new
reviewed baseline; the comparison fails rather than matching unlike cases.

For stable scheduled or self-hosted CI, use the CLI commands above with a reviewed
baseline from that exact environment. The Box experiment results must not be
used as a baseline for unrelated GitHub-hosted runners.

Run the validator tests and Rust checks with:

```sh
python3 -m unittest discover -s experiments/tests -v
cargo fmt --manifest-path experiments/pty/Cargo.toml --check
cargo clippy --manifest-path experiments/pty/Cargo.toml --locked --all-targets -- -D warnings
```

The local runner writes to its result and cache directories. The CI workflow
uploads evidence as repository workflow artifacts. Neither modifies sibling
packages. Archives and build products stay in the ignored cache; measured reports
belong in `docs/experiments`, and architectural decisions belong in `docs/adr`.
