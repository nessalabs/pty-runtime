# Helper image materializer: independent DDD and ownership review

Reviewed 2026-09-08. Applied repository AGENTS.md, coding standards and ADR 0005. Scope: portable child-owned image materialization, constructor wiring, backend contract and retained constructor regression. No builds, tests, workload execution or production edits by this reviewer. Native crash work remains excluded.

## Result

No P1/P2 DDD, dependency-direction or ownership blocker found in the selected implementation. Source review supports the chosen ownership fix. Post-fix execution, error-path fixtures and full gate remain required; this is not their result.

## Ownership and lifecycle

`image_materialize.rs` stays entirely in infrastructure and returns existing portable ProcessError categories. `image.rs` retains ownership of the private directory and publishes HelperImage only after materialization succeeds. The parent never opens the executable inode writable. The short-lived materializer opens its writer after fork and does not fork again, preventing unrelated forks in the caller process from inheriting that writer. This is stronger than serializing library spawners, which cannot coordinate external Command launches.

The child reads precomputed CString/immutable bytes and uses raw open/write/fchmod/close/errno/_exit. It does not invoke Rust allocation, logging, locks or destructors. The write loop handles partial writes and EINTR and rejects zero progress. All early failures lead through _exit, closing the writer even when child_write returns before explicit close. Successful close precedes success exit. Path creation remains exclusive/no-follow, with private directory ownership and final mode 0500.

`write` saves and blocks the calling thread's signal mask before fork. The parent restores its prior mask immediately after fork, captures fork errno before restoration, and reaps any successful child even when restoring the mask fails. The child retains the blocked mask so inherited application signal handlers do not run during its filesystem operations. Standard platform atfork handlers still apply, as the public Rustdoc acknowledges. The implementation must not be described as bypassing caller atfork behavior.

`reap` waits for the exact owned PID, retries EINTR, and accepts only actual exited/signaled terminal states. A stopped trace state cannot be mistaken for completion or trigger early image publication. Signals and unknown exit codes map to Io. Backend host validation occurs before image construction and rejects SIGCHLD auto-reaping; the existing no-competing-reaper host contract covers this additional child. No global signal disposition is changed.

The parent-owned image value is created before materialization, so failure drops it and attempts file/directory cleanup after the child has been waited. This retains the existing best-effort Drop cleanup semantics, not a new guarantee that filesystem deletion cannot fail. Backend Rustdoc truthfully states that filesystem operations and child waiting have no fixed wall-clock bound. No timeout can guarantee interruption of arbitrary kernel filesystem I/O, so no such claim is added.

## Error representation

Private exit categories 0=success, 2=NotFound, 3=PermissionDenied, other=Io are adequate for this minimal contract. They preserve the public categories expected from the previous std::io conversion without leaking raw errno or inventing domain lifecycle states. CString validation yields InvalidCommand before fork. A signal-killed child or unknown status cannot fabricate successful materialization.

An errno pipe would add descriptor ownership, protocol/read failure and correlation obligations. It is not necessary for the selected portable public result. If later investigation requires stage/errno diagnostics, introduce a fixed-size private infrastructure message and retain existing public categories; avoid logs or allocation after fork. Lack of detailed child errno is an observability limitation, not an ownership blocker for this patch.

## Test evidence and remaining checks

Read `docs/verification/image-fork/red/command.log`: the actual-constructor regression fails with ExecutableFileBusy/errno26 under the old writer ownership. The fixture releases/reaps the unrelated blocked pre-exec process before reporting the assertion; it executes the same image after release and requires exit125, distinguishing the writer conflict from invalid image bytes. Current wiring observes the constructor handoff without reopening the image. The new implementation may safely create its writer later in a separate child while that external child remains blocked.

This reviewer did not execute GREEN. Pending materializer tests should cover byte equality/mode, existing-path rejection without truncation, missing parent, stable permission-category mapping where feasible, restoration of the caller signal mask on success/failure, exact child reaping and failure cleanup. Do not infer executed coverage from these recommendations. Linux ETXTBSY reproduction does not independently execute the macOS implementation.

## Primary documentation consulted

Linux execve documents ETXTBSY for executables open writable; fork documents inherited open file descriptions and restricted postfork calls in multithreaded processes. The signal-safety list supports the selected raw operations. These establish OS contracts rather than claiming a test result:

- [execve](https://www.man7.org/linux/man-pages/man2/execve.2.html)
- [fork](https://man7.org/linux/man-pages/man2/fork.2.html)
- [signal safety](https://man7.org/linux/man-pages/man7/signal-safety.7.html)

## Selected SHA-256

| Source | SHA-256 |
| --- | --- |
| `crates/infrastructure/src/process/image_materialize.rs` | `24408bb54499d5fa3e34347fa9d588149bdefbb2f32ab25a69e4c1d539d7b7b5` |
| `crates/infrastructure/src/process/image.rs` | `045767b8c2228a5a9a785522e7a2a0063aa1b711c1e088f9531e93b2e48c076f` |
| `crates/infrastructure/src/process/mod.rs` | `2aca92ecb352154704c76416ee76ad510dc787f0b7626580f6ff32f09ff68490` |
| `crates/infrastructure/src/process/backend.rs` | `2d2639b977a56c58f608c94fd2e1b3e6c21afd797b583da8fe57df9e91bf4ea2` |
| `crates/infrastructure/src/process/signals.rs` | `82b5e900864ffe08233aed8db2b6af494fb2d1955c9721b3fbf90f6bf5e94ec4` |
| `crates/infrastructure/tests/fixtures/process_image_fork.rs` | `01f34d4ffeb42024d1e46f95e370072340b374ec6d85beed3e040cc40d7b9bfc` |

## Final actual-open seam re-review

Independently re-read the strengthened formatted source and fixtures. No new P1/P2 DDD/ownership blocker found. The hashes below supersede the earlier implementation/fixture hashes for this final review; historical execution logs must still be matched to their own source identities.

The observer now follows the actual successful writable open in child_write, rather than a constructor handoff approximation. In the materializer child, Hook::opened uses only getpid and raw write/poll/read on the inherited test socket. It cannot take the RefCell/closure branch because the hook records the parent's PID before fork. The constructing parent receives readiness before invoking the Rust callback, starts the unrelated blocked pre-exec process while the materializer really owns the writer, then releases the child. Therefore the callback observes the relevant interval without itself opening or retaining the image descriptor.

The explicit parent-PID branch permits a parent-writer mutation to call the same actual-open hook and execute the observer synchronously while the parent owns the writer. This makes the test sensitive to writer ownership, not merely sensitive to relocating a hook. Static inspection supports this mutation design; this reviewer has not executed the final mutation or claimed its RED result. Initial RED/GREEN logs are not automatically evidence for this strengthened seam's hashes.

Observer panic is caught in the parent, the child is released, and reap completes before resume_unwind. Observation I/O failure is similarly propagated only after reap. The hook's bounded socket waits are fixture coordination, not a new production filesystem deadline. Hook/observer code is cfg(test) only; ordinary write calls use None and non-test compilation contains no callback contract or additional protocol. The three materializer tests independently assert exact arbitrary bytes, final mode0500, calling-thread mask restoration, missing-parent NotFound and existing-destination Io without truncation. SignalMask compares signal membership instead of padding and restores only the current thread's prior mask. These tests are meaningful but still do not cover every filesystem/wait failure or prove cross-platform execution by static inspection.

The production ownership conclusion remains: CString and bytes are prepared in the parent; only the postfork child opens the target writer; parent restores its mask and waits for actual terminal state; private exit categories translate to existing ProcessError values. No application/domain/native-pointer dependency or public capability was added. No local tests/builds were run for this refresh. Final macOS/Linux regression, parent-writer mutation and gate evidence remain coordinator responsibilities.

### Final reviewed SHA-256

| Source | SHA-256 |
| --- | --- |
| `crates/infrastructure/src/process/image_materialize.rs` | `2f3d02bcc547feabed0d65398abe34ccbd37381ae6c71696cfa75e0e02917604` |
| `crates/infrastructure/src/process/image_materialize_hook.rs` | `6b882c754d990d04891cc391c1a92473964d1283ad1b042d0049791794499ce7` |
| `crates/infrastructure/src/process/image.rs` | `505fc327cc1583cda99d165705fe7efadafb2eb1f1b2bea48480ee6ccd3a3ac6` |
| `crates/infrastructure/src/process/mod.rs` | `2aca92ecb352154704c76416ee76ad510dc787f0b7626580f6ff32f09ff68490` |
| `crates/infrastructure/src/process/backend.rs` | `2d2639b977a56c58f608c94fd2e1b3e6c21afd797b583da8fe57df9e91bf4ea2` |
| `crates/infrastructure/src/process/signals.rs` | `82b5e900864ffe08233aed8db2b6af494fb2d1955c9721b3fbf90f6bf5e94ec4` |
| `crates/infrastructure/tests/fixtures/process_image_fork.rs` | `a41846b814763e33a2bc7bb9edc6315cd418586d9b6bb616073b4b3149a94f81` |
| `crates/infrastructure/tests/fixtures/process_image_materialize.rs` | `4d3afffec148af414720ce4d648f377cbe2665fb5ede161831f224adaca9412a` |
