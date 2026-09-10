# Loop 2 domain, ownership, and boundary review

Reviewed 2026-09-08 against the working tree before the second push. Sources:
application runtime/process/terminal modules, domain identity/replay/process and
terminal contracts, infrastructure registry/process/terminal adapters, facade,
and `scripts/native/README.md`. Production files were not edited by this reviewer.
The separately named `tests/ddd_spawn_failure.rs` is independent review evidence.

This review does not establish G1 or G2 completion. Native projection is not yet
connected to runtime sessions; automatic parking and encrypted storage remain
future work. Existing native adapter tests cannot prove the integrated path.

## Findings requiring fixes or explicit tracked follow-up

### L2-D01 — P1: observers of a failed starting session never complete

`crates/application/src/runtime/owner.rs:76–97` registers the context before the
backend call, correctly allowing early events. However, `lookup` can expose that
context while spawn is pending. The failure branch only rolls back registration.
An already returned session or attachment retains the context, whose exit, drain,
and supervision error remain empty permanently. Its completion future never
resolves, and removal from the registry cannot wake it.

Executed `cargo test --locked --no-default-features --test ddd_spawn_failure`:
**1 failed, 0 passed**. A barrier-controlled injected backend exposes the window
then returns `NotFound`. The registry is correctly empty afterward, but the old
handle has no terminal outcome. This deterministic failure requires no OS timing.

Publish an explicit failed-start domain transition and wake existing observers
before rollback, or make Starting lookup an explicit unavailable state. Preserve
actual exit versus failure distinction. The former strategy retains the current
early-observation API and should satisfy the added test. ADRs 0001, 0004, 0005.

### L2-D02 — P1: domain lifecycle authority is implemented in application

`crates/application/src/runtime/options.rs:109–136` owns lifecycle state and the
completion predicate; `context.rs:206–230` performs first-event-wins transitions;
`session.rs:42–46` mutates cancellation intent. These are the aggregate rules
ADR 0005 explicitly assigns to domain. The domain currently contains data values
and replay but no session lifecycle aggregate. Correct Cargo edges do not repair
this behavioral placement.

Move concrete lifecycle state/transitions/completion rules into a domain type.
Application should translate events into those methods, coordinate ports, and
wake observers. Make failed-start and terminal supervision transitions explicit;
test the same domain methods used by production callbacks. Do not create an
interface for this internal state machine. The physical mutex can remain an
application implementation detail for short state protection, without allowing
OS/engine scheduling or port calls under it.

### L2-D03 — P1: runtime input admission has no independent global budget

`crates/application/src/runtime/options.rs` exposes global session/observer/replay
limits but no input byte or admitted-operation limits. `session.rs:38–39` forwards
input to the process port; infrastructure `process/session.rs:90–103` checks only
each session's configured caps and immediately copies. Different sessions can
choose arbitrarily large `input_bytes`, `input_slots`, and `input_chunk` values.
Session-count admission alone does not express or enforce the independent runtime
input budget required by ADR 0002.

Add application-owned global admission before copying; transfer a reservation
through the admitted operation and release it on actual completion/failure,
including when its caller drops the future. Keep byte counts and operation counts
distinct so empty writes cannot defeat an operation limit. Add concurrent
cross-session saturation and abandoned-wait tests. Port implementers must receive
an enforceable reservation contract rather than trust a comment about bounds.

### L2-D04 — P1: blocking spawn work monopolizes the sole control/reaper worker

`crates/infrastructure/src/process/supervisor.rs:58–82` starts up to eight children
synchronously before servicing existing process controls. `start` performs
canonicalization, directory traversal, PTY allocation, and `Command::spawn` through
`spawn::launch`. A slow filesystem/exec handshake can block cancellation and
reaping of every existing session. Batching eight attempts bounds the count,
not the execution time of one attempt.

Use a bounded spawn worker path whose result is adopted by the supervisor, with
explicit ownership and shutdown cleanup for a child created after shutdown began.
Keep cancellation/reaping independently serviceable. Verify through a controlled
blocked-spawn seam while an existing process receives cancellation; do not depend
only on fast local executables. This is a structural responsiveness risk established
by the call path, not a measured timeout from this review. ADRs 0001, 0002, 0004.

### L2-D05 — P2: owner Drop uses graceful shutdown instead of immediate termination

`crates/application/src/runtime/owner.rs:121–124` calls the ordinary shutdown
path from Drop. The backend joins its supervisor, which observes configured
termination grace before escalation; `ProcessLimits` allows up to one day.
Thus dropping an owner can block for a caller-configured day for an ignoring
child. ADR 0001 separates graceful shutdown from immediate best-effort owner Drop.

Define distinct graceful and immediate backend termination semantics. The
application/facade Drop must invoke the latter while ensuring reaping remains
owned. Test a signal-ignoring fixture with a long graceful setting and verify
Drop does not wait out that setting. Document whether final worker joining can
still block on OS cleanup. No such latency experiment was run by this reviewer.

## Positive boundary and ownership observations

- Core types do not expose native handles or third-party error types. Command
  metadata is compact, bounded, literal, and Debug-redacted; filesystem
  canonicalization remains in the process adapter.
- Session state and the live process collaborator are separate fields. Process
  calls occur after the collaborator mutex is released; inspected application
  code does not invoke process/repository/terminal ports under its state mutex.
  Waking extracted observers occurs after state unlock.
- Identity registration is atomic in the repository, includes retained completed
  entries, and removal rechecks the lifetime. This protects replacement IDs
  against stale removal.
- Replay cursor validation, exact gaps, and absolute byte accounting stay in
  domain. Application coordinates the shared replay quota and charges reported
  allocated capacity. Retained logical bytes and capacity are explicitly distinct.
- Input admission precedes the per-session copy; partial write outcomes remain
  distinct from child consumption, and cancellation does not use the input queue.
- Native ownership is exclusive and transferred as one owner; C callbacks do not
  reenter Rust. Domain-facing terminal values and compatibility errors are engine
  neutral; native conversion stays in infrastructure.

## Native READY limitation and integration requirements

The native README records full-state reference evidence showing lost history
when mutation occurs before restoration completes. Advertising
`mutation_during_restore: false` and rejecting feed/resize unchanged with
`HistoryIncomplete` is an honest capability boundary. It is preferable to
claiming complete preserved state from an active-screen-only comparison.

The forthcoming application coordinator must stage bytes and controls under
finite independent bounds, finish history work incrementally, and apply each
event exactly once after Complete. READY may provide an observation, but cannot
be reported as history integrity verification or live-output readiness for this
adapter. Update the owning ADR's operational wording and qualification record
around this measured constraint; do not hide it in implementation notes alone.

Do not feed native code while holding the lifecycle/replay mutex. Keep one
serialized terminal operation owner and publish results through lifetime/control
generation checks. Full parser staging must backpressure only its own reader;
control and reaping must remain independent. Terminal query replies need the same
ordered writer as user input, including admission when input queues are full.

The terminal port accepts caller-supplied checkpoint metadata but cannot validate
runtime session identity or processed byte position by itself. That remains the
application coordinator's responsibility at the exact encoding boundary. Store
and protector integration must authenticate those fields and retain the source
until decoding completes. Existing adapter-only checks do not prove this.

## Follow-up evidence

Resolve L2-D01 and rerun its deterministic regression. Re-review domain extraction
and global admission against actual production call sites. Add blocked-spawn,
immediate-Drop, early cancel-before-process-publication, and shutdown-during-spawn
tests. Run the mandatory mechanical gate after fixes and preserve its output.
No G1/G2 pass is granted by this document; the full ADR performance, platform,
stress, and soak gates remain outstanding.

## Fix re-review

Reinspected the subsequent working tree on 2026-09-08. The original findings
above remain as the historical review; the table below is their current status.

| Finding | Current status | Inspected implementation and executed evidence |
| --- | --- | --- |
| L2-D01 failed-start observers | Resolved | Application `owner.rs` now records failure and failed drain, extracts/wakes observers through `context.record`, then rolls back registration. `cargo test --locked --no-default-features --test ddd_spawn_failure --test input_reservation` passed the failed-spawn regression (1 test). No actual exit is fabricated. |
| L2-D02 domain lifecycle authority | Resolved for the present raw lifecycle | `crates/domain/src/session/mod.rs` owns status, completion, cancellation admission, exit/drain conflict checks, failure recording, options and errors. Application callbacks call these methods. Process collaborators remain outside domain. Projection/parking transitions are not yet implemented or approved. |
| L2-D03 global input admission | Resolved for public session writes | `Runtime` owns separate byte/slot quotas; `Session::write` acquires `InputLease` before crossing the port; `write_reserved` transfers it into queued `Input`. Input destruction clears bytes, then releases owned fields/lease before sending acknowledgement on success or cleanup. The input-reservation test passed (1 test), proving cross-session rejection and retention of quota after the caller drops its future. Direct process-adapter calls retain their documented per-process bounds; the runtime API supplies global admission. |
| L2-D04 blocking spawn stalls controls | Resolved for existing-process service | One bounded spawner performs launch work, and the supervisor adopts prepared children separately. RAII pending-child/admission owners clean late children and release reservations. `cargo test --locked -p pty-runtime-infrastructure --no-default-features spawn_barrier_tests` passed 2 tests: existing input/resize/cancel/reaping while a launch is blocked, and immediate shutdown plus late-child cleanup. |
| L2-D05 graceful delay on owner Drop | Resolved for signal escalation semantics | Application Drop invokes `shutdown_now`; backend sets immediate shutdown; supervisor kills without configured grace. The immediate-shutdown barrier test uses a one-day grace and checks actual exit within two seconds, then verifies the late child is already reaped. Application Drop dispatch was inspected; this test invokes the backend immediate path directly. |

The above commands executed on this macOS arm64 host. Filtered test binaries
running zero tests are not counted as additional passes. These tests establish
the named fixes, not Linux execution, integrated projection, stress, or soak.

Shutdown completion still has no wall-clock bound when a launch is blocked in
an uninterruptible OS/filesystem operation. The adapter now documents that
existing processes are reaped independently before joining the spawner. This is
an honest remaining limitation, not a successful proof of bounded shutdown in
every OS condition.

### L2-D06 — P1 milestone gap: foreground job-control cancellation remains absent

`crates/infrastructure/src/process/signals.rs::signal` now deliberately targets
the anchored original process group and child only. This avoids the unsafe
unanchored `tcgetpgrp`-then-signal race. Its safety rationale and host requirement
against competing reapers are explicit and preferable to claiming invalid PID
reuse protection.

However, ADR 0001 requires cancellation of the foreground job-control group as
well. A foreground job can be in another process group while remaining in the
same terminal session; this is not necessarily a deliberately escaped session.
The backend's narrower documentation does not complete that original requirement.
Track and qualify a safe foreground-group ownership mechanism, or obtain an
explicitly reviewed scope decision before claiming the owning milestone. No
foreground-group proof was executed by this reviewer. This remains open for G1.

No additional production boundary blocker was identified in the inspected fixes.
The native capability contract remains honest about disallowing mutation until
history completes. Integrated projection, bounded parser staging, ordered query
replies, parking/store/protector orchestration, and their race qualification are
still not approved by this raw-runtime re-review. New checkpoint files observed
in the working tree were outside this fix re-review's scope.
