# Review loop 2: raw runtime and concrete adapter slice

Recorded 2026-09-08. This is an implementation/review checkpoint, **not a passed
G1, G2, G3, or release milestone**. The full [requirements ledger](requirements.md)
remains authoritative. Foreground job control, projection/parking orchestration,
observer state transfer, event-stream integration, and release workloads remain
pending. No 128-projected-session capacity or soak result is claimed.

## Implemented and exercised scope

- Raw Unix session owner: reserved identity/lifetime, retained repository entries,
  bounded independent replay cursors and gaps, transient input reservations,
  cancellation intent, separate actual exit/drain/failure, shutdown and drop.
- Unix adapter: one interruptible dedicated reader per live PTY; shared readiness
  supervision and bounded spawn worker; pinned-root command policy; bounded ordered
  writes and OS resize. Current cancellation controls the original anchored group.
  Foreground-group coverage still requires the separately reviewed guardian work.
- Actual pinned Ghostty adapter: static native build, exclusive FFI ownership,
  allocation/feed/query/resize/destruction, bounded continuations/checkpoints,
  replies, compression, and complete retained-history verification. READY is an
  observable restoration milestone; this pin must finish history before mutation.
- XChaCha20-Poly1305 checkpoint protection and private immutable filesystem storage:
  authenticated identity/descriptor metadata, bounded reads/commits, abandoned-byte
  charges, plaintext cleanup, namespace identity checks, and constructor rollback.
- Shared bounded scheduler, separate blocking executor, monotonic clock/capacity
  signal: coalesced wakes, per-registration serialization, finite admission,
  callback/destructor panic containment, and documented worker-shutdown exception.

## Executed validation

| Command/result | Evidence | Scope |
| --- | --- | --- |
| `python3 scripts/gate.py`: pass, macOS arm64 | [final log](loop2/macos-final-gate/command.log), [metadata and exact source hashes](loop2/macos-final-gate/metadata.json) | Architecture/size negative tests, core independence, formatting, real native bootstrap, strict Clippy, workspace/all-feature tests, raw-only tests, Rustdoc warnings, experiment validator and fixture checks |
| `cargo +1.85.0 test --locked --workspace --all-targets --all-features --target-dir target/msrv-loop2`: pass, macOS arm64 | [log](loop2/macos-msrv/command.log), [metadata](loop2/macos-msrv/metadata.json) | Runtime, native and adapter tests on declared minimum Rust; subsequent final-gate source differs only by Cargo trailing whitespace and checkpoint-port documentation |
| Earlier complete macOS gate: pass | [log](loop2/macos-gate/command.log), [metadata](loop2/macos-gate/metadata.json) | Retained pre-final documentation/whitespace revision |
| Linux x86_64 full gate | Pending execution/report; no pass claimed in this checkpoint | Fresh Box source copy; initial transfer contained AppleDouble metadata and failed source scanning before tests; cleaned transfer is being tested |
| Foreground primitive: 150 cases each on macOS arm64/Linux x86_64, zero failures | [source/raw files](foreground-prototype/), [design and exact limits](../reviews/foreground-control-design.md) | Isolated mechanism prototype only; no production guardian, owner-abort cleanup, interactive-shell or resource qualification claim |

`record_validation.py` records command, platform, toolchain, base commit, every
source-file hash before execution, command output, and whether sources changed
while the command ran. Both final macOS records have successful commands and
unchanged source manifests. The implementation changes are based on foundation
commit `1c5d17e311ae24e8da53b36aa60e399b070c1974`; exact uncommitted source identities
are in each record and become part of this review commit.

## Independent adversarial reviews and fixes

- [DDD](../reviews/loop2-ddd.md) and [follow-up](../reviews/loop2-followup-ddd.md):
  session invariants moved into domain, application retained orchestration,
  injected provider contract became envelope-independent, abandoned storage charges
  became explicit. C-D01 and C-D02 independently closed.
- [Organization](../reviews/loop2-organization.md) and
  [follow-up](../reviews/loop2-followup-organization.md): separated lifecycle,
  supervision and native responsibilities; enforced native/test file-size inventory;
  fixed failed-constructor namespace ownership. Isolated restrictive-umask regression
  proves rollback of the newly created directory.
- [Correctness](../reviews/loop2-correctness.md),
  [checkpoint findings](../reviews/checkpoint-adapter-correctness.md), and
  [follow-up](../reviews/loop2-followup-correctness.md): independent regressions
  reproduced reader failure cleanup, directory replacement, plaintext cleanup,
  impossible scheduler capacity, and destructor-panic worker loss; re-review tests
  pass after fixes. Four checkpoint and four scheduler adversarial tests remain
  in the package, alongside existing adapter contract tests.

No reviewed adapter-slice blocker remains in these follow-ups. Full ADR blockers
remain explicit: the host must not reap managed children; the original-group
control path does not yet satisfy arbitrary foreground jobs; unexpected guardian
loss needs a proved cleanup mechanism before that proposed topology is adopted.
The private checkpoint parent must remain trusted against concurrent replacement.
Injected blocking providers and OS process creation must eventually return;
uninterruptible external operations cannot be given an invented completion bound.
