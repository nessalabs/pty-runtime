# Capacity fixture implementation evidence

ADR 0002 calls for saturated throughput, independently reported input RTT, explicit producer blocking, bounded state and five-repeat baseline comparisons. Paced 10 MiB/s acceptance alone cannot establish capacity. The matrix now retains controlled cases and adds raw/projected unpaced active producers with finite per-phase byte caps. Implementation and metric definitions are in `scripts/release/LOAD.md`.

This is implementation verification only, **not a release baseline**. The macOS host concurrently ran existing observer qualification; these debug no-default/event-stream small-population timings must not be compared with release baselines. No Linux workload was run. Projected saturation and five full repetitions are not run here. Frozen candidate-2 evidence was not modified.

`config-before.log` preserves the failing configuration regression before implementation. `tests-green.log` records all five focused tests passing: explicit saturation/cap validation, real stalled nonblocking socket deadline and exact partial-write bytes, idle/unpaced distinction and censored cap, delayed readiness, actual-window skew. `clippy.log` is focused warnings-as-errors verification. Build/test commands use `CARGO_TARGET_DIR=work/load-capacity-test-target cargo {build,test,clippy} --locked --no-default-features --features event-stream --example release_load` (Clippy adds `-- -D warnings`).

The raw and controlled smokes use 4 resident/2 active, warmup 1 second and measurement 2 seconds. Both complete with exact raw bytes/gaps and zero final logical resources. The controlled smoke offers 1 MiB/s. The censored run uses 2/1, warmup 0, duration 1 second and `--producer-bytes 100`: it emits cap_exhausted=true and exits 1 with the explicit censored-trial error, with no successful throughput result. A real Darwin smoke exposed POLLNVAL when polling the /dev/tty alias; opening ttyname_r(stdout)'s actual slave fixes this while preserving a separate output file description.

`python-smoke/` is the final driver path: `python3 scripts/release/load.py --binary work/load-capacity-test-target/debug/examples/release_load --case capacity-raw --smoke --output work/load-capacity-review/python-smoke`. It exits 0 and accepts the null offered-rate field for unpaced output. Source hashes there bind the executing artifact; `source.json` includes final explanatory safety-comment changes. Full gate and independent specialist reviews remain the coordinating task's acceptance work.
## Subsequent reviewed release build and projected smoke

`cap-boundary/` retains the final capped-byte control-flow correction and its
static-proof limitation; the independent review verifies it resolved.
`final-source-build/` records a successful release build after the Python
tri-state reporting correction, with an unchanged complete source inventory.
`projected-smoke/` records a completed 4-resident/2-active projected unpaced trial,
1-second warmup and 2-second measurement. Its exact accepted ledger is
224,149,058 bytes over a 2.000437-second producer window (112,050,046 bytes/s).
All recorded latency targets and correctness checks passed. This earlier smoke
predates the Python tri-state correction but uses the same final Rust producer;
its own complete source identity is retained. It is not a full-repeat baseline.

Independent capacity correctness, fixture organization, Python contracts and
reporting re-review are recorded under `docs/archive/reviews`. The scope excludes the
retained candidate-2 dominant-load failure and other open release requirements.
