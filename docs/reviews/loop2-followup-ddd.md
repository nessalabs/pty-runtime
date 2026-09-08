# Loop 2 follow-up: checkpoint and scheduler domain boundaries

Reviewed on macOS arm64 against the working tree based on
`1c5d17e311ae24e8da53b36aa60e399b070c1974`. This reviewer did not author the
checkpoint or scheduling implementations and made no production edits. The
foreground section below is an ADR consistency check of this reviewer's earlier
design study, not a second independent approval of its mechanism.

Outcome: **C-D01 and C-D02 are closed. No new blocking dependency-direction or
domain-ownership finding in the checkpoint/scheduler adapters.** This does not
approve integrated parking, G3 completion, or foreground-control implementation.

## Checkpoint finding closure

**C-D01 — closed.** `crates/infrastructure/src/checkpoint/file.rs:80` checks only
nonempty opaque bytes and a metadata bound; the store no longer imposes the
built-in cipher's 40-byte overhead. That envelope check remains in
`checkpoint/protector.rs::open`. The independent
`tests/checkpoint_opaque_envelope.rs` commits, reads and deletes a 17-byte
opaque fixture without invoking the built-in protector. This test passed again
in this review. It proves storage-format independence, not the security of a
17-byte encryption scheme.

**C-D02 — closed.** `crates/domain/src/checkpoint/mod.rs:57` now distinguishes
committed, abandoned and in-flight bytes. The application store contract at
`crates/application/src/checkpoint/mod.rs:31` says failed cleanup retains a
charge rather than promising reservation release. The file adapter subtracts
committed plus abandoned bytes from remaining capacity at `file.rs:84` and
records failed deletion separately at `file.rs:105`.

The injected `failed_orphan_cleanup_retains_a_distinct_quota_charge` test writes
10 bytes of a reserved 60-byte object, then fails both writing and cleanup.
It verifies zero committed/in-flight bytes, 60 abandoned bytes, a remaining
pending file, and rejection of another 60-byte commit under a 100-byte quota.
Drop subsequently removes the orphan. This is direct failure evidence for the
contract, not merely a renamed counter.

One non-blocking documentation clarification remains: the shared store port does
not explicitly state its nonempty-ciphertext precondition, although the built-in
store enforces it (`file.rs:81`). State that precondition at
`ICheckpointStore::commit` so injected implementations have the same rejection
contract. This does not reopen the removed cipher-envelope coupling.

## Domain and port ownership

Cargo metadata reports no dependencies for `pty-runtime-domain`, and only
`pty-runtime-domain` for `pty-runtime-application`. Crypto, randomness, file
operations, thread workers, condvars, and platform mechanics remain in
infrastructure. Core APIs expose local checkpoint references, ordering metadata,
capacity/error models, and standard-library ownership/time types.

`CheckpointKey` and `CheckpointDescriptor` preserve lifetime, operation
generation, byte position and control generation. The protector authenticates
these values and rejects independently supplied expected metadata that does not
match. The store keeps those values opaque; it neither interprets terminal
state nor chooses parking eligibility. Provider-issued `object_id` identifies
an immutable storage object and is not substituted for the session lifetime or
replay cursor.

`ProtectedCheckpoint::new` is intentionally usable with untrusted provider
bytes: it is not an authentication constructor. The application must call the
protector's `open` before terminal restoration. Likewise, cloneable opaque
buffers do not themselves enforce global staging/immutable-pin budgets; those
leases belong to runtime admission and must survive abandoned waits.

The current domain session model contains process/replay policy but no parking
transition aggregate. No application parking coordinator is present. Therefore
these adapters cannot yet prove generation revalidation after commit, atomic
model release, stale-result cleanup, or ordered READY/history restoration. Those
rules must be added to domain/application state rather than hidden inside file
storage or scheduler entries.

## Scheduling additions

The application declares the relevant external capabilities:
`IClock`, `IWorkScheduler`, `IBlockingExecutor`, and `ICapacitySignal` in
`crates/application/src/scheduling/mod.rs`. `IScheduledWork` and `IWorkHandle`
express serialized work and coalesced wake ownership. These are meaningful
replaceable execution boundaries; there is no service locator or interface
mirroring an unrelated domain aggregate.

`StdWorkScheduler` owns fixed workers, bounded registration slots and one timer
per registration. Its internal registration generation protects recycled slots;
it is deliberately distinct from a checkpoint's domain generation. A callback
returns scheduling intent; the adapter does not decide whether a session is
eligible to park or whether a stored generation is current.

`BoundedBlockingExecutor` counts queued plus active jobs before admission
(`scheduling/blocking.rs:67`) and runs providers outside its queue mutex
(`blocking.rs:93`). This permits application orchestration to keep storage work
outside terminal/session control ownership. It does not turn blocking provider
calls into cancellable operations: the port correctly retains accepted work
when a caller drops its wait, and shutdown requires those operations to finish.

Worker-call shutdown has explicit semantics in the current application port
(`scheduling/mod.rs:53` and `:62`): request closure without self/cross-joining;
external shutdown supplies a completion barrier. The callback-shutdown test
executes this path, then performs external shutdown. Applications must not use
a callback's returned shutdown call as evidence that all workers are joined.

The clock supplies elapsed monotonic time rather than wall-clock values.
Capacity notification generations preserve notification-before-wait events;
callers still own and recheck their domain capacity predicate. The adapter's
condvar and OS `Instant` scheduling do not leak executor/native types into the
portable domain. Injected deterministic clocks do not, by themselves, advance
the infrastructure scheduler's real timers; deterministic orchestration tests
should supply both the clock and a controllable scheduling implementation.

## Foreground design versus the ADRs

The proposed guardian remains an infrastructure implementation of the existing
process boundary. Actual workload PID and actual workload exit must be converted
at that boundary; guardian PID, anchor membership, process status wire records
and OS handles must not enter domain state. A guardian restart must not reuse a
session lifetime or silently relaunch its workload.

The design preserves ADR 0001's foreground-group requirement instead of
relabeling root-group-only cancellation as sufficient. It also preserves
separate exit and drain outcomes. However, it is still a design: its prototype
uses synthetic same-session groups, not integrated shell job control, and it
has not implemented the Rust helper's lifecycle or protocol in the runtime.

**FG-D01 — P1 completion gap, explicitly acknowledged by the design.**
`docs/reviews/foreground-control-design.md:245` records that unexpected guardian
death can orphan the workload. ADR 0001 requires worker abortion to trigger
cleanup. Reporting supervision failure is necessary but cannot be treated as
proof that the owned workload has been cleaned up, or justify dropping its
remaining ownership records. A qualified guardian-failure cleanup mechanism and
its evidence are required before this path passes G1. The live guardian's
owner-channel-EOF cleanup is a different scenario and does not close this gap.

A per-PTY guardian and temporary anchors also change the measured worker/process
resource model. Keep them explicitly charged as runtime infrastructure, not
hidden within user workload costs or the few-KiB owner-control target. Retain
bounded helper admission, dedicated reader accounting, original performance
cases and final lifecycle/soak requirements. The design already calls for this
qualification; it is not an implementation-completion claim.

## Executed evidence and scope

The following commands passed during this review:

```sh
cargo test --locked -p pty-runtime-infrastructure --no-default-features \
  --test checkpoint_contract --test checkpoint_opaque_envelope \
  --test checkpoint_adversarial --test scheduling_pool --test scheduling_blocking
cargo test --locked -p pty-runtime-infrastructure --no-default-features \
  --lib checkpoint::file::tests
cargo test --locked -p pty-runtime-infrastructure --no-default-features \
  --lib scheduling::
cargo metadata --locked --format-version 1 --no-deps
```

Results: **13 checkpoint tests and 10 scheduling tests passed**. They cover
opaque-provider interchange, authentication corruption/metadata failures,
private-file behavior, failed orphan cleanup accounting, concurrent disk quota,
coalesced wakes, non-overlap, registration limits, timer wake, panic containment,
callback shutdown, blocking-job admission/drain, early capacity notifications,
and external worker joining/release.

No Linux commands, integrated native parking, signed helper execution, full
performance repeats, lifecycle/reconnect qualification or soak were run by this
review. The earlier cross-platform foreground prototype remains separately
scoped evidence in its own design report. The full mechanical gate and other
specialist reviews remain required before the next commit/push.
