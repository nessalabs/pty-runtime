# Additional executed raw G1 acceptance evidence

Executed 2026-09-08 on macOS arm64. **Nine public-runtime tests pass**: four new
acceptance cases plus five existing raw/polled-wait regressions. Focused Clippy
with warnings denied also passes. This is bounded functional G1 evidence, not a
completed G1 milestone, native projection qualification, stress or performance.

## Exact source and execution

The active worktree was temporarily unbuildable while projection modules were
being introduced. At the owner's direction, execution used an isolated detached
worktree at **`cc69ab47aeb73d717a702edeb14540605463e743`**, applying only the
new root tests and fixture additions. No production file or dependency changed
in that worktree. The active root retains identical test sources for the next
integrated gate; that later gate must rerun them against the new implementation.

- [Source hashes, host/toolchain and commands](data/g1-raw-acceptance/source-manifest.json)
- [Exact test/fixture patch](data/g1-raw-acceptance/test-changes.patch)
- [Four new acceptance cases](data/g1-raw-acceptance/1-test.log)
- [Focused Clippy output](data/g1-raw-acceptance/2-clippy.log)
- [Five existing fixture regressions](data/g1-raw-acceptance/3-existing-regressions.log)

The patch and manifest together identify every changed file relative to the
recorded base commit. Existing support/tests derive from that commit. Tests ran
with `--locked --no-default-features`, explicitly exercising raw sessions. A
native-cache symlink was available but these commands do not enable Ghostty.
No Linux behavior was executed in this proof. Earlier active-worktree attempts
failed to compile because projection module files were not yet present; those
attempts do not count as executed tests or runtime failures.

## Requirement mapping and remaining scope

IDs refer to [the complete requirements ledger](requirements.md).

| Rows | Actual evidence | Scope still unproven |
| --- | --- | --- |
| G1-03 | `raw_registration`: 16 threads released by one barrier contend for the same ID; exactly one gets a new session and 15 get ExistingSession. A real child appends a launch marker, proving one OS execution. Another 16 calls after completion all reject without new markers. Explicit forget permits exactly a second launch and fresh lifetime; old replay cursor rejects; lookup PID agrees. | Larger/randomized shutdown/spawn races, injected partial-registration failures, metadata/resource cleanup and 10,000 cycles; one barrier scenario is not exhaustive concurrent proof. |
| G1-01, G1-02 | `raw_child_contract`: real fixture verifies stdin/stdout/stderr are terminals, opens `/dev/tty`, verifies canonical cwd from a dot-containing path, and checks shell-looking arguments literally. Empty environment contains only the explicit synthetic override. Inheritance preserves expected HOME without printing it, removes PATH, and applies an override after same-name removal. | No complete launch-path/symlink race suite in these new tests; no foreground process-group proof. Launch path policy remains distinct from a child filesystem sandbox. |
| G1-02, G1-15 | Child reports initial 81×25 dimensions, waits for input, then reports 97×33 after public resize acknowledgement. Exact output excludes submitted synthetic input, proving initial echo is off. Separate test merges NUL/non-UTF8 stdout/stderr bytes in exact write order. | Projected OS/model resize consistency, failed resize outcomes, native replies, saturation fairness and control latency. This is OS resize only. |
| G1-04 | `raw_completion`: ordinary nonzero code 19, signal death 15 and a descendant-held endpoint with real code 23 stay distinct. Retained output precedes completion; completion remains stable across later reads and wait. On macOS this descendant fixture reaches EOF after leader exit. | Linux test branch expects its separately observed drain-timeout behavior but was not executed here. No macOS timeout assertion is manufactured to match Linux. Foreground-group/escaped-descendant cleanup remains pending. |
| G1-05, G1-13 | Existing `raw_adversarial` rerun actually polls attachment and completion futures to Pending before dropping them. One-observer admission is reused; cursor unchanged; later bytes arrive. | Repeated randomized read/eviction/completion races, 100,000 attach/detach operations, resource plateaus and concurrent projected snapshots. |
| G1-02, G1-05, G1-06, G1-10, G1-13 | Existing `raw_runtime` rerun proves synthetic prompt/input non-echo, detach/reconnect process identity, exact retained suffix/gap, completed ID behavior, bounded observer/registry admission and owner-drop completion for its fixtures. | General resource exhaustion, failed worker/spawn cleanup, full process-group behavior, quantitative memory accounting and sustained stability remain separate gates. |

The tests use a local Rust fixture or explicitly selected `/bin/sh` and
`/bin/stty`, synthetic byte/input data and a temporary launch-counter file. They
run no real login flow, credentials, model calls or paid resource. Parent
inheritance values are compared in the fixture without printing them.

All new source files remain below the coding standard's 350 nonblank-line limit.
No production edits were made for this evidence. The full mechanical gate and
independent DDD/organization review still apply before the next main push.
