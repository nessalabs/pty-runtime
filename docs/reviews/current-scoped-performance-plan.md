# Current bounded performance gate plan

Prospective plan only: no build or benchmark has run for this final source state.
The coordinator owns the build lane and will signal when the final changes are
stable. Do not overlap performance with compilation, coverage or another load.

The sole production delta in the currently reviewed work is seven equivalent
journal PoisonError method references. Other changes are tests and measurement
scripts. The minimum acceptance workload is the existing mandatory gate workload,
without a new benchmark implementation or a repeated baseline cohort:

- `scripts/performance.py`: its existing release runtime_performance test, one
  four-MiB-per-session trial at one and 16 concurrent projected sessions, real
  PTY/helper/native parser and exact raw/parser validation.
- `scripts/program_status_performance.py`: its existing five release trials for
  each ordinary, oversized, queries, saturated and one-byte workload, 25 records.
  This is included because the mechanical gate already requires it, not as a
  claim that the journal change affects that domain path.

Run each once with its own fresh evidence output. Preserve every raw result and
failure; no replacement run to select a favorable result. If the final full gate
already executes these exact workloads on unchanged sources and bindings, audit
that evidence rather than duplicating the runs. This is bounded acceptance
measurement, not the full release repetition/soak contract, a fresh <=10% paired
baseline comparison, or evidence of a speedup from the journal cleanup.

At collection time, freeze the actual final source inventory/hash using the
existing performance identity function. Record git revision plus dirty source
identity; HEAD alone is insufficient. Bind the actual driver/test/workload source,
lockfiles, rustc/platform, verified native source/archive/static library and the
built helper. Prepare release executables before timed runs; retain Cargo JSON
and its unique selected test/example executable, hash that executable and its
runtime artifacts before and after. Confirm the executed Running path matches
preparation and that Cargo does not compile during the collected trial. Compare
full artifact populations as well as known hashes. Source or artifact drift
invalidates the collection; freeze again before any later separately recorded run.

Sanitize inherited measurement configuration: remove coverage/profiling flags,
LLVM_PROFILE_FILE, RUSTC_WRAPPER/RUSTC_WORKSPACE_WRAPPER, Cargo coverage/target
overrides and guardian-image override. Use the verified native build source
explicitly. Record removed environment names, not secrets. The final executable
must be an ordinary optimized release build, not the V12 measurement compiler.

Accept runtime evidence only with the required two ordered session records,
success=true, positive elapsed time, four MiB payload per session, exact raw and
parser bytes (4,194,326 at one session; 67,109,222 at 16), normal exit, no timeout,
and zero observed descendants after shutdown. Preserve timing, FD, RSS and other
resource fields with their existing scope limitations; process RSS is sampled
and excludes helper/producer processes. Require all 25 expected positive domain
records and unchanged source identity. Inspect raw logs and load context rather
than treating driver exit zero alone as sufficient evidence. Final review will
report actual values and remaining limits; this plan makes no performance pass.

## Existing mechanical-gate observations and binding limitation

The final mechanical gate passed in 145.149 seconds. Its 353-file source
manifest matches continuous matrix02 and current disk. Both mandatory
performance scripts ran and exited zero with before/after/current source digest
`81df424f31d1ef69d1514b49021921a42ec61b2c0a3ee596f5e0360e53c20054`.
Platform is macOS 26.6 arm64, rustc 1.98.1 (48a229cea, LLVM 22.1.8).

Original evidence, preserved without replacement:

- `work/runtime-performance/20261012T091931Z-17c9048b`: runtime payload intervals
  46.534750 ms at one session and 650.856666 ms at 16 sessions. Exact raw/parser
  totals are 4,194,326 and 67,109,222 bytes, both success=true, no timeout, normal
  exit, five FDs and zero observed descendants after shutdown. Processing RSS
  samples are 11,583,488 and 146,866,176 bytes with the stated process-only,
  non-peak limitation. Cleanup intervals are 1.054625 and 1.288000 ms.
- `work/program-status-performance/20261012T091935Z-dc9a9048`: all 25 expected
  records are present. Five-trial medians in milliseconds are ordinary 0.731125,
  oversized 11.797292, queries 5.993833, saturated 13.798500 and one_byte
  10.432917. Full individual observations remain in stdout.jsonl.

These are valid source-bound observations, but the execution did not capture
the prospective before/after executable/native artifact bindings. Runtime Cargo
compiled three crates for 3.06 seconds before entering the fixture; its payload
timer excludes compilation, but this is not a prewarmed comparative cohort.
The gate did not persist launch environment; the coordinator reports its default
shell had RUSTC_WRAPPER=sccache and the other discussed coverage/flag overrides
unset. Current executable hashes alone would be retrospective and cannot prove
what was present before execution. No missing binding is invented, and this
entry does not call the prospective artifact-binding requirement satisfied.

The coordinator authorized one separately recorded prospective clean-environment
execution of the same two existing scripts after all competing builds finish.
It will remove sccache and coverage wrappers/flags, prepare the actual release
executables with Cargo JSON, bind the complete selected source/tool/native/helper
artifacts before and after, and confirm executed paths with no timed Cargo
compilation. This is evidence-gap correction, not a replacement for the earlier
values or permission to repeat until favorable results. No fresh baseline
comparison, <=10% claim, speedup or release-soak pass is implied. The coordinator
owns execution; the independent reviewer has launched no benchmark.

## Prospective bound performance01: independent final audit

**PASS for the bounded performance evidence gate on the current source state.**
The separately authorized collection is retained at
`work/coverage-closure/final-bound-performance01`; the original mechanical-gate
measurements above remain untouched. This verdict does not pass full coverage,
a comparative regression threshold or the release repetition/soak requirements.

The launch record and inspected orchestration strip sccache and coverage
wrappers/flags/profile overrides, explicitly select the verified native source,
and prepare the two existing release workloads before measuring. Cargo JSON
selects exactly one runtime test executable and one domain example executable.
Independent anchored parsing of the actual Running lines matches both selected
paths. Both saved Mach-O section dumps lack __llvm_prf_ sections. The actual
runtime/raw.txt and domain/stderr.txt contain no Compiling, Checking or Dirty
line during measurement; both scripts exit zero.

Independently rehashed all 353 source entries and 1,607 bound files: before and
after manifests agree and all current contents match. Source digest remains
`81df424f31d1ef69d1514b49021921a42ec61b2c0a3ee596f5e0360e53c20054`.
Re-enumerated all 355 release artifact files (no symlinks) and all 1,245 selected
native source/library files; populations match exactly. The native archive,
build stamp and static library, built guardian helper, rustc/cargo, performance
drivers and ignored orchestrator are bound. The source selection is explicit;
it is not a claim that unrelated native documentation/cache files were measured.

Runtime executable SHA-256 is
`97ceaabcc0afc0c6410f9ee0f2f366636d358a9b3ea2f4cdd36528dd6dd9f94c`;
domain executable SHA-256 is
`d157249de52103018c749634dd03b906c28dc6cb56e2ed3451c889b6c10127e9`.
Platform remains macOS 26.6 arm64, rustc 1.98.1.

| Runtime observation | One session | 16 sessions |
| --- | ---: | ---: |
| Payload interval, ms | 50.825167 | 594.252750 |
| Startup, ms | 229.162292 | 241.634125 |
| Cleanup, ms | 3.168375 | 1.478833 |
| Exact raw/parser bytes | 4,194,326 | 67,109,222 |
| RSS after processing, bytes | 11,468,800 | 150,978,560 |
| FDs after shutdown | 5 | 5 |
| Observed descendants after shutdown | 0 | 0 |

Both ordered runtime records have release build, four MiB payload per session,
success=true, normal exit and no timeout. RSS is a point sample of the test
process, excluding helper/producer processes; peak RSS and PSS remain unmeasured.
The runtime command took 1.390 seconds overall; that is distinct from payload
intervals. No cross-run causal conclusion follows from differences against the
earlier gate observations.

All 25 domain records retain the required workload/trial order and positive
byte/time values. Five-trial elapsed milliseconds, with medians in parentheses:

- ordinary: 0.268042, 0.278875, 0.245625, 0.222750, 0.223000 (0.245625).
- oversized: 9.364375, 9.495458, 9.523875, 9.333791, 9.134958 (9.364375).
- queries: 5.938417, 5.542458, 5.526750, 5.939083, 6.149791 (5.938417).
- saturated: 13.379750, 13.410125, 13.718334, 13.201583, 13.580625 (13.410125).
- one_byte: 9.952375, 9.895625, 9.971958, 9.958208, 9.974959 (9.958208).

P3 orchestration limitation for future reuse: the script's no-Compiling guard
scans outer domain-measure output, while the domain script stores Cargo stderr
under domain/stderr.txt; the executed-path guard is also a substring check.
These guards should inspect the real nested log and require one anchored Running
line if the operator is reused. For this retained run, independent inspection
performed both stricter checks successfully. No frozen evidence was edited and
no repeat was requested to change measurements. No unresolved P1/P2 affects
this bounded evidence verdict; no reviewer build or benchmark was launched.

## Final test-fault scope: independent gate and performance audit

`work/coverage-closure/final-test-fault-gate01` passes the complete mechanical
gate in 160.800 s. Its 361-source manifest exactly matches the final
fault-matrix03 manifest and current source. The ordinary gate's runtime and
domain observations remain retained at
`work/runtime-performance/20261012T115105Z-7202f257` and
`work/program-status-performance/20261012T115111Z-061c2320`; they have not been
replaced by the later run. Runtime timings were 47.588917 ms and 513.818875 ms
for one and 16 sessions. That Cargo invocation compiled two crates for 4.38 s
before running the fixture. Domain medians were 0.727292, 13.764417, 5.788792,
13.314292 and 9.925416 ms in ordinary/oversized/queries/saturated/one_byte order.

The separately identified prospective run is
`work/coverage-closure/final-test-fault-bound-performance01`. Independent audit
passes for its stated bounded scope. Its 361 sources and 1,607 artifact/input
hashes match before, after and current files; the exact release population is
355 files, with 1,245 bound native source/library files plus the fixed inputs.
The verified native archive/build stamp/library and actual helper image are
included. Source digest for both mandatory performance scripts is
`463c4fc0a3424cac96d3bad11af1441e8ece0c45ea8c517cb3624231915ceb09`, matching current
source and the ordinary gate observations.

The runtime executable hash is
`59f30dc502ce14c6d5dd921d07c988ce720fc793ef41c2d148bf0e381628105c`; the domain
executable hash is
`d157249de52103018c749634dd03b906c28dc6cb56e2ed3451c889b6c10127e9`.
Each is the unique prepared Cargo JSON executable. Independently inspected the
actual nested runtime and domain logs: one exact anchored Running path each,
and no Compiling, Checking or Dirty entries. Neither image has LLVM profiling
sections. The final ignored orchestrator now performs those stronger checks
itself, resolving the earlier P3 reuse limitation. Wrapper/profiling variables
are cleared before preparation and measurement.

Production test-seam absence was also inspected: the release infrastructure
and helper depfiles contain none of the test fault modules; `nm` on the actual
runtime and helper image reports no new test fault or regression symbols.
The cfg(test) module/argument erasure is unchanged. This is supporting binary
and compile-input evidence, not reliance on symbol absence alone.

| Prospective runtime observation | 1 session | 16 sessions |
| --- | ---: | ---: |
| 4 MiB/session processing | 48.803708 ms | 459.202292 ms |
| Startup | 238.574583 ms | 204.843625 ms |
| Cleanup | 1.019375 ms | 1.241542 ms |
| Raw and parser bytes, exact | 4,194,326 | 67,109,222 |
| Process RSS after processing | 11,452,416 B | 156,434,432 B |
| FDs after shutdown | 5 | 5 |
| Descendants after shutdown | 0 | 0 |

Raw runtime records match the two records parsed from the retained log. All 25
domain records have exact workload/trial order and positive sizes/timings.
Five-trial elapsed milliseconds, with medians in parentheses:

- ordinary: 0.590250, 0.589417, 0.589542, 0.576916, 0.482375 (0.589417).
- oversized: 15.298458, 12.594041, 11.697125, 10.618750, 9.940833 (11.697125).
- queries: 6.158667, 6.469708, 6.304875, 6.145375, 6.213333 (6.213333).
- saturated: 13.258208, 13.201042, 13.146250, 13.072583, 13.094041 (13.146250).
- one_byte: 10.006292, 10.142375, 10.036083, 9.941916, 10.131042 (10.036083).

The scoped evidence and mechanical gate pass with no unresolved P1/P2 in this
review. These are one runtime observation per session count and five domain
trials per workload. They do not establish a speedup, a paired relative
regression bound, full release repetitions or soak readiness. RSS values are
point samples for the test process, excluding helper/producer processes and
not a peak. The separate unchanged 100% coverage gate still fails. No reviewer
build, benchmark, or favorable-result rerun was performed.
