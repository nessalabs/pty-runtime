# Loop 2 adversarial correctness review

Reviewed 2026-09-08: application runtime/context/attachment/session/quota;
domain replay/process outcomes; registry and public facade; Unix spawn, reader,
writer, supervision and exit-watch adapters; Rust Ghostty owner/projection/state
and C owner/checkpoint bridge. This report is a review of a work-in-progress
implementation. **G1 and G2 remain pending** and this is not release proof.

## Findings

| Priority / status | Code evidence | Finding and required disposition |
| --- | --- | --- |
| P1, reproduced; fix pending re-review | `crates/infrastructure/src/process/io.rs::reader`; `supervisor.rs::run/control` | Reader panic/read failure sets `reader_done` but never admits child cancellation. Supervisor retains the process until actual exit, so a long-running child is left with no reader and may block forever on output. Existing callback-panic test's `sleep 1` masks this. Independent `process_adversarial` test uses `printf trigger; sleep 60`: reader failure was observed, but no exit in two seconds; test cleaned up with shutdown before failing. Require bounded termination/reaping on unexpected reader loss, preserving failed drain separately. |
| P1, host contract correction pending | `supervisor.rs::signal/control/reap`; `watch.rs` | The unreaped-root-PID safety comment assumes exclusive reaping, while `reap` attempts to recover after another host waiter reaps the process. A competing waiter, SIGCHLD ignored, or SA_NOCLDWAIT can release the PID before the supervisor notices; `control` can signal first. Merely calling try_wait before kill does not close the race. Root is adding rejection of automatic-reap signal dispositions and explicit host ownership requirements. Revalidate these as a necessary embedding contract; they do not make competing waiters safe. |
| P1, unresolved platform proof | `supervisor.rs::signal`, different foreground group path | `tcgetpgrp` followed by numeric `kill(-foreground, ...)` does not anchor a distinct foreground process group's identity. The root child's unreaped PID does not reserve the foreground job's PGID. Checking SID/PGID first is another userspace TOCTOU. Linux individual pidfds with validated session membership can address individual signal identity but need a bounded dynamic-member policy; macOS requires separate proof. Kernel last-master hangup safely targets the tty's actual foreground group but changes drain behavior and delivers HUP, not arbitrary termination escalation. Do not extend the root-PID safety claim to arbitrary foreground groups without a proven mechanism. |
| P1, tracked by root/DDD reviewer | `runtime/owner.rs::spawn` failure rollback and early lookup | A context published before backend spawn may be held by a lookup/attachment even after registry rollback. It needs terminal failure/drain facts to complete those waits. Root is fixing the separately reproduced failed-spawn context hang; this review does not independently mark it resolved. |
| P2, being implemented; re-review required | Input admission across runtime and process queues | Original per-session queue caps bound only that session. Runtime-wide input bytes/slots need leases held by the actual queued input until release, including after the caller drops its returned future. Root is adding global quota wiring. A lease held only by the future would release too early; a held completed future must not pin queue capacity. |

The spawn path also performs filesystem/exec setup on the shared supervisor in
the inspected version. A blocking startup can delay cancellation/reaping of
unrelated sessions. Root reports this is being moved off that path. Dedicated
readers alone do not establish control responsiveness while spawn is blocked.

## Independent tests and evidence

- Added `crates/infrastructure/tests/process_adversarial.rs`.
  `cargo test --locked -p pty-runtime-infrastructure --test process_adversarial`
  **failed** on macOS arm64 before the reader-failure fix. The test waited two
  seconds for automatic termination after an observed reader callback panic,
  then explicitly shut down to ensure cleanup before asserting. Failure and
  timeout are retained here, not omitted from evidence.
- Added `tests/raw_adversarial.rs`, explicitly polls attachment and completion
  waits to Pending before dropping them, then proves admission reuse with one
  observer permit, unchanged cursor, and subsequent byte delivery. Its first
  run encountered an in-progress compile failure from the quota edit in
  `SessionContext::read`; no result is claimed until rerun.

## Native review scope and limits

The native owner has exclusive `&mut` operations, heap-stable allocator/callback
userdata, no Rust callback unwinding boundary, and Drop destroys the decoder
before the checkpoint byte owner. The source buffer remains retained through
incremental history restoration. Checked compatibility and final decoder source
offset reject wrong-engine input and trailing bytes. Existing native tests cover
specific corrupt/truncated snapshots; READY alone intentionally does not certify
integrity of remaining history. Failed restoration marks the projection unusable.

The independent full-state oracle described in `scripts/native/README.md` found
history loss when mutating after READY. The current capability explicitly rejects
feed/resize/checkpoint/compression until history completes. This is honest adapter
behavior; the forthcoming application integration must stage bytes and ordered
controls losslessly under separate budgets, keep control/reaping responsive, and
measure full wake latency. It does not discharge ADR 0003's integrated restoration
requirements merely by returning a READY view.

Native `view_bytes` bounds copied text; admitted grid count bounds cell metadata
separately. `native_bytes` accounts requested allocator bytes, excluding allocation
overhead and the C owner; Rust checkpoint/reply buffers are separately capped.
Their configured ceilings are allocated before native operations, so temporary
peak reservations and repeated snapshot pins still need runtime accounting.
No claim of encrypted storage, integrated replies, native crash isolation, or
runtime-native performance is established here.

## Remaining ownership and qualification caveats

Replay reservations bound allocator-visible capacity independently of logical
retention. Retained old session handles intentionally retain context/replay after
registry forgetting; resource evidence must account for those live handles.
Returned pages are caller-owned copies, not part of ongoing replay ownership.
Watcher cleanup on dropped pending futures now has a genuinely-polled independent
test rather than only dropping an unpolled future. It is still not evidence for
100,000 attachment operations or cancellation races under saturated projected I/O.

The private repository and passing adapter tests do not replace the complete
[requirements ledger](../verification/requirements.md): actual integrated
platform execution, retained process/drain facts, failed-spawn cleanup,
foreground job control, pressure/fairness, encrypted default parking, full
performance repeats, 10,000 lifecycle cycles, and the 12-hour soak remain gates.

## Re-review after process fixes

Executed again 2026-09-08 on macOS arm64:

- `cargo test --locked -p pty-runtime-infrastructure --test process_adversarial`
  passes (one test). `io::reader` now records reader failure and the lifecycle
  owner responds with SIGKILL and actual reaping. The reproduced lost-reader
  cleanup finding is resolved for this regression.
- `cargo test --locked --test raw_adversarial` passes (one test). An actually
  polled pending read and completion wait can be dropped with cursor preservation
  and observer permit reuse. The earlier compilation block is resolved.
- Re-read `process/signals.rs`: it now rejects ignored SIGCHLD and
  SA_NOCLDWAIT and requires the embedding host never reap managed children.
  That corrects the previously implicit root-PID assumption. Changing host signal
  disposition or using a competing waiter remains outside that explicit contract.
- Re-read `process/signals.rs`: unsafe numeric foreground-group signalling is
  removed; only the anchored root process/group is signalled. The unsafe operation
  is resolved, but **ADR foreground job-control coverage remains unimplemented**.
  Removing that behavior is not proof that the full G1 cancellation gate passes.

The original failure evidence remains above. Startup callbacks, native integration,
all platform coverage and full stress/performance gates remain separately pending.
