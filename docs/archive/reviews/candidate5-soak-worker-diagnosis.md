# Candidate 5 soak: typed Worker error diagnosis

## Evidence and scope

The frozen Linux `71c1d6f42bbd1c547f747fdd7d20ed69e3000c4e` full soak failed with exit 1 and `Error: Projection(Worker)`. This is an ordinary Rust error returned through the fixture's `Result`, not evidence of a native crash. No panic or abort is recorded. This review reads only the retained log and Rust fixture/application/scheduler paths; it performs no workload, remote operation, native C/crash investigation, runtime edit, or rerun.

Artifact: `docs/verification/release/candidate5-soak-failed.jsonl`, SHA-256 `065d240dcde9fd43c07083bd074bd00495f9d5e77ffb7ccab29d9e61d5ffd2b0`. Identity records an empty source diff, full non-smoke soak, and binary SHA-256 `84034c829428d1c7db9e30de564bcf7b1ce0b132821069d1804bfeb512b7e739`.

The final progress record is turn 576 at 145.623819018 seconds: 590,875 verified bytes, 1,768,421 explicitly gapped bytes, and 560 parked observations. The preceding resource record has eight active sessions, two stored checkpoint slots, zero staged bytes/slots and requests, and zero failed-operation/cleanup counters. These are preceding samples, not evidence that no operation failed afterward. The two parked projected sessions had not reached the fixture's first 180-second cold wake. No successful full-soak completion or final cleanup record exists.

## Narrowing from fixture ordering

`examples/release_support/soak.rs` prints `soak_progress` only after its every-16-turn metrics, projected view on child 1, checkpoint on child 3, and projected resize on child 1 all succeed. Those operations therefore succeeded at turn 576.

Immediately afterward, the every-32-turn branch calls `harness.spawn(turns % 64 == 0)` followed by `harness.finish(transient, turns % 64 != 0)`. At 576 this creates a **projected** temporary session and finishes it **gracefully**. `Harness::finish` requests child exit, waits for process completion, checks exit/drain assertions, and calls `Runtime::forget`. There is no stage marker between these calls. The immediate next error-capable projection boundaries are thus projected transient creation/binding and projected cleanup during `forget`.

The next every-16-turn metrics record would precede the next periodic view/checkpoint/resize group, but is absent. The log does not identify the temporary session lifetime, whether spawn returned, or whether forget began. Do not label either creation or cleanup as the proven origin. The failure's timing and fixture sequence make them the first paths to instrument. The raw process completion future itself returns completion or `Internal`, not a synthesized `Projection(Worker)`; fixture assertions would produce a panic instead of this returned error.

## Rust paths producing Worker

| Boundary | Worker-producing mechanism |
| --- | --- |
| Projected creation | `projection/coordinator.rs`: caught panic in protector size calculation or terminal factory creation; scheduler registration error collapsed to Worker; initial/bind wake failure |
| Scheduling wake | `coordinator.rs::wake`: missing handle or any `IWorkHandle::wake` error becomes Worker. `infrastructure/scheduling/pool.rs` rejects wakes when the scheduler is closed or the matching entry is absent/closed |
| Close/forget | `runtime/owner.rs::forget` calls `runtime/projected.rs::close`, which immediately propagates non-Capacity errors from `ProjectionCoordinator::close`; the latter calls `wake` after publishing Closing. A stored cleanup failure also propagates |
| Already admitted work | `worker.rs::failed` marks Worker after a scheduler-contained Rust panic. `admission.rs` and `stream_end.rs` can mark Worker after wake failure |
| Provider completion | `io.rs` converts a caught provider-job panic to `IoResult::Failure`; `completion.rs` converts unexpected/failure result variants to Worker for commit/read/transfer/delete |
| Other internal conditions | Missing saved source in `io.rs::start_read`; caught Rust panic in the application terminal-call wrapper; shutdown fallback or contained destructor/resize cleanup failure in `teardown.rs` |

These sources explain why the type alone does not identify an operation. Absence of a recorded panic makes a caught-panic explanation unsupported; it does not by itself prove a scheduler failure. Provider rejection normally retains its typed error, and blocking-executor submission rejection maps to Capacity rather than Worker.

## Concrete cleanup race hypothesis

There is a source-visible interleaving worth checking if the fixture localizes this to `forget`:

1. `ProjectionCoordinator::close` changes policy to Closing, releases the core mutex, closes the journal/rejects queued operations, then calls `self.wake()?`.
2. A worker already running or already scheduled, for example by final output drain, can observe Closing and complete `worker.rs::cleanup` before that wake.
3. Cleanup records Closed and its durable outcome, releases services, and takes/drops the work handle. The scheduler subsequently removes the finished entry.
4. The closing caller's `wake` can observe no handle, or a cloned handle whose scheduler entry is now closed/removed, and return Worker.
5. `runtime/projected.rs::close` returns that error without rechecking the now-durable close outcome in its error branch.

This is a plausible spurious close error even when cleanup succeeded. It needs no native fault or Rust panic. It is **not established as the observed failure**, because there are no fixture stage markers or close-state diagnostics. Do not change error handling or suppress Worker on this evidence alone; a real cleanup failure must remain visible.

## Minimal next diagnostics

Add fixture-only stage context around the transient branch: turn, projected/cancel flags, sequence/lifetime when known, and `transient_spawn`, `transient_exit_request`, `transient_wait`, `transient_forget`, each with start/success/error. Preserve the original error as the source of any contextual error. This distinguishes the two leading boundaries without modifying runtime behavior or adding per-byte logging. On forget error, capture the transient's projection status and process status before harness destruction, marking snapshot failure separately and retaining the original error. Do not use raw PTY payload, command arguments, or environment as diagnostics.

If localized to creation, distinguish factory/protector, scheduler register, initial wake and bind wake while preserving the underlying SchedulingError. If localized to forget, capture whether the wake failure saw a missing handle versus a rejected scheduler wake, and the durable close outcome immediately afterward. A deterministic Rust scheduler-controlled test could force cleanup between Closing publication and the caller's wake and check that the durable cleanup outcome is honored; that is a proposed follow-up, not executed in this review.

No automatic rerun is warranted. Preserve this failed acceptance attempt and choose the next bounded diagnostic only after reviewing the stage evidence available. The 12-hour soak remains unmet.
