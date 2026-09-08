# Actual-runtime release repetition and soak

Build the default Ghostty composition with `cargo build --locked --release
--example release_stress`. Run from the repository root:

```sh
python3 scripts/release/stress.py repetition --output work/release/repetition.jsonl
python3 scripts/release/stress.py races --output work/release/races.jsonl
python3 scripts/release/stress.py soak --output work/release/soak.jsonl
```

Defaults are 10,000 real spawn/exit-or-cancel cycles plus 100,000 attach/drop
cycles, 256 seeded concurrent race rounds, and twelve hours of mixed actual-runtime
soak. `--smoke` explicitly reduces counts/duration and is never release acceptance.
`soak --parking-smoke` runs 85 seconds to test the default 60-second parking deadline;
it remains a reduced pilot, not twelve-hour evidence. `soak --restore-smoke` runs
270 seconds to prove an initial park, a cold restore at 180 seconds, and re-parking. The Rust example uses a
separate Unix control socket for deterministic fixture readiness and exit/output
barriers. Every lifetime has its own deterministic ASCII pattern encoding its full seed; retained ranges and
explicit gaps account for exact stream positions without logging payloads.

Repetition uses one runtime, emits and verifies one seeded 4096-byte burst in
every exit/cancel cycle, and explicitly forgets each completed ID. Each temporary
observer drops before quiescent resource checks, stressing replay allocation and
release as well as process lifecycle. Attach churn
checks unchanged lifetime/PID/cursor and proves all four observer reservations can
be reacquired afterward. Quiescent checkpoints block on the external driver while
it confirms no remaining child or zombie and records owner resources. Final owner
FD count must equal baseline. Enabled aggregate diagnostics additionally assert
zero active sessions, retained replay, and every runtime/projection reservation at
quiescent checkpoints, and emit all counters/budget usages during soak. RSS time series remain available for plateau review;
the script does not claim plateau from a short run or allocator high-water marks.

Races cover cancel versus exit, resize versus exit, attach versus eviction, and
shutdown versus spawn with shared thread barriers and a recorded deterministic
seed. Soak keeps eight sessions (four projected), leaves two projected sessions
idle through automatic parking, produces output on four, exercises views,
checkpoints and ordered resize, and periodically spawns/exits/cancels transient
sessions. Every 180 seconds each cold projected session wakes through output,
proves parser catchup and native residency, obtains a view/checkpoint, reattaches
at its exact cursor, and returns to automatic parking. Parking deadlines use each
session's last mutation time. It rejects unexpected process/projection failures
and verifies each session's final bytes plus gaps against its own commanded total
before aggregate accounting; identical totals cannot hide cross-session delivery.

The driver records source HEAD, diff and source-inventory hashes, exact executable
SHA-256, platform, command, and whether the run is reduced. Use a frozen source and
fresh binary for acceptance. The driver copies that binary into a private
temporary directory before launch so all self-executed fixtures keep the exact
same image even if Cargo later replaces its build output. A 45-second no-progress watchdog bounds blocking
spawn/shutdown/fixture operations; asynchronous operations also have 15-second
limits. Failure kills only the driver's exact unreaped child; guardian owner EOF
retains descendant cleanup authority. Periodic RSS/PSS/FD/CPU samples cover the
actual owner/helper/workload process tree, separately from this Python observer.
PSS is unavailable on macOS. Full-soak resource analysis excludes the first 120
seconds as warm-up. Transient process disappearance during sampling is recorded.

A successful driver result verifies exercised assertions and quiescent descriptor
cleanup. Release acceptance additionally requires independent review of the
post-warm-up resource time series, matching platform/build identity, the full
counts/duration, and the remaining ADR performance and failure-injection evidence.
