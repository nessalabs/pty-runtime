# Loop 2 follow-up organization and design review

Reviewed 2026-09-08 against the current uncommitted tree. This is an independent
review of process, terminal and checkpoint additions; the reviewer authored the
scheduler adapter and explicitly excludes it from this independent assessment.
Read `coding_standards.md`, the preceding loop-2 organization review, checkpoint
correctness review and foreground-control design. No production code changed.

## New blocking finding

**P2 — failed checkpoint namespace construction has no cleanup owner.**
`crates/infrastructure/src/checkpoint/filesystem.rs:39` creates a directory, but
ownership is not represented by a returned `Directory` until line 60. The
`openat` failure at lines 50–51, metadata failure at line 55, and permissions/
owner rejection at lines 57–58 return without removing the newly created
namespace. `FileCheckpointStore::Drop` cannot run because its constructor never
completed. The filesystem helper itself has no construction guard or Drop.
A restrictive umask can make the requested 0700 mode fail validation; descriptor
exhaustion between creation and open is another possible trigger. Repeated
constructor failures leave untracked namespaces outside store quota accounting.

Acquire rollback ownership immediately after successful creation, keep it until
construction succeeds, and perform cleanup through the existing anchored parent
and documented trusted-parent policy. Preserve the existing refusal to remove
an unrelated replacement. Add an isolated child-process restrictive-umask test
or deterministic post-mkdir failure injection proving failed construction leaves
no namespace. This finding is established by source inspection; this review did
not execute an injected construction failure. Status: open, sent to root.

## Decomposition and ownership assessment

The earlier supervisor growth concern remains resolved. Current nonblank counts:
`process/supervisor.rs` 104, `lifecycle.rs` 124, `registration.rs` 91,
`spawner.rs` 127 and `backend.rs` 195. They represent meaningful boundaries:
request admission/launch, transfer and rollback, event polling, single-child
lifecycle and backend worker ownership. `Admission` follows the admitted process
through queues; `PendingChild` kills/reaps a launched child that cannot transfer
to the supervisor; `OwnedProcess` owns reader joining and final child cleanup.
No arbitrary helper extraction or generic service locator was introduced.

Terminal ownership remains cohesive. `terminal/mod.rs` (136 nonblank lines)
constructs/destroys the sole native owner and controls compatibility;
`state.rs` (128) implements mutable engine operations; `projection.rs` (152)
converts engine state into portable cells/modes/palette. Raw ABI declarations
stay private in `ffi.rs`. Native files separately own allocation/lifetime,
checkpoint encoding and view conversion. Borrowed checkpoint bytes remain in
the Rust owner until decoder destruction/completion. The adapter exposes the
real mutation-during-restore limitation instead of concealing it in scheduling.

Checkpoint decomposition is appropriate: `file.rs` (206 nonblank lines) owns
immutable publication/accounting, `filesystem.rs` (160) owns anchored OS
operations, and `protector.rs` (114) owns authenticated protection and zeroizing
buffers. Crypto and filesystem details stay behind application-owned ports.
Store serialization is explicitly a storage mutex, not a native/session lock.
That division is sound provided integrated callers use bounded blocking work.
The new construction leak above is a gap in RAII coverage, not a reason to
collapse these responsibilities into one implementation.

Application `runtime/context.rs` is 233 nonblank lines and still handles raw
replay, watcher registration and process-event translation. It has not absorbed
parking/native state. Add integrated projection in its own coordinator, as the
prior review requested; do not grow context into an all-purpose session engine.
Facade composition currently wires raw process/repository use cases. Presence
of terminal and checkpoint adapters does not prove integrated G2/G3 behavior.

## Previous findings rechecked

- Facade process/repository/terminal port signatures remain explicitly exported,
  including `SessionContext` and the engine-neutral terminal module. No new
  wildcard port export was observed. Checkpoint/scheduler public composition
  is subsequent work, not silently inferred from internal crate APIs.
- Coding standards now explicitly cover hand-written Rust, C and header files,
  including build helpers and tests. `scripts/tests/test_sizes.py` injects an
  oversized actual build script, C source and header and requires rejection.
  The earlier follow-up gap is resolved by inspected implementation and an
  executed negative test.
- The replacement-namespace deletion issue has an anchored parent/inode check
  in `Directory::remove_namespace`; its remaining concurrent-rename limitation
  is documented as requiring a trusted parent. This does not address the new
  partial-construction path, which never calls that method.
- Protector now takes plaintext into `Zeroizing` before fallible validation and
  clears spare capacity before possible reallocation. That responds directly
  to the prior buffer-ownership findings; independent behavioral/cryptographic
  review remains the authority for those guarantees.

## Foreground design feasibility and completion blockers

`docs/archive/reviews/foreground-control-design.md` provides a concrete topology that
preserves workload/guardian identity separation and gives foreground signalling
an owned group-member anchor. Keeping the guardian as workload parent assigns
reaping to one owner; using a fresh executable avoids installing an ordinary
Rust event loop inside a multithreaded host's pre-exec child. Bounded control
records and two admitted anchors fit the existing separation of process control
from dedicated byte readers. This is a plausible implementation direction, not
an implemented capability.

The packaging/build/guardian protocol should be focused infrastructure modules
and a dedicated helper package, with target-aware image construction and an
explicit version handshake. Domain and application should receive actual
workload identity/status and typed supervision faults, not helper topology or
file-descriptor details. Keep helper exit distinct from workload exit throughout
`OwnedProcess` conversion; the current direct `Child` representation cannot be
reused unchanged with a guardian and still report the promised process facts.

The design itself identifies outstanding guardian-SIGKILL cleanup, executable
packaging/signing, interactive shell job control, output-endpoint inheritance,
protocol backpressure and resource qualification. Those remain blockers to
calling foreground cancellation/G1 complete. Current `process/signals.rs:20–26`
still signals the anchored original group/direct child only; the backend
Rustdoc states that limitation. The isolated anchor prototype neither replaces
production code nor proves the integrated topology. Added per-session helper
processes must be included in resource measurements, not hidden behind the
shared-supervisor label. No fresh prototype or remote-platform execution was
performed by this review.

## Executed evidence and disposition

`python3 -m unittest discover -s scripts/tests -v` passed all 12 tests on macOS,
including current workspace architecture acceptance and all three oversized
native/build-source rejection cases. An independent nonblank source inventory
found no file-size violation in the reviewed subsystems. The root must run and
preserve the complete mandatory gate after all concurrent changes settle; these
focused checks do not substitute for it or native/process qualification.

Organization verdict: decomposition is sound, with the P2 constructor-cleanup
finding still open. Integrated foreground/projection/parking and full release
proof remain unfinished. Do not mark this review or any ADR complete until the
cleanup fix is independently re-reviewed and the relevant full gate passes.

## Independent re-review of constructor fix

Root added `ConstructionGuard` immediately after successful `mkdirat`, before
`openat` or metadata validation. The borrowed anchored parent/name outlive its
Drop, every later error keeps it armed, and successful handoff disarms it before
moving the descriptors into `Directory`. Creation failure before mkdir success
never arms cleanup. This covers the reported incomplete-ownership window under
the same expressly trusted-parent condition as construction itself.

Added an independent regression in
`crates/infrastructure/tests/checkpoint_adversarial.rs`:
`failed_namespace_construction_rolls_back_under_restrictive_umask`. It executes
only that test in a separate process, applies umask 0100 so requested 0700 loses
owner execute permission, requires constructor rejection, and verifies the new
namespace is absent. The parent also checks absence and removes any residual
fixture before failing, without altering its own process-global umask.

Executed `cargo test --locked -p pty-runtime-infrastructure --test
checkpoint_adversarial` on macOS: all four tests pass, including this new
construction regression and the existing foreign-replacement preservation test.

**P2 constructor-cleanup finding: resolved by independent source inspection and
executed regression.** The open disposition above is superseded. No remaining
blocking organization defect was identified in the reviewed adapter slice.
Foreground integration, G2/G3 orchestration and release qualification remain
unfinished; this re-review does not mark any ADR or full gate complete.
