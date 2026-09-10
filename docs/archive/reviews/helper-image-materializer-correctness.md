# Helper image materializer correctness review

**No production P1/P2 found. The prior P2 regression-hook placement gap is resolved in source.** This refreshed review is static only; platform execution and the unchanged-hook parent-writer mutation result remain pending evidence from the parent task. No build, workload, or production edit was performed by this reviewer. This is not an all-gate pass.

## Prior finding and resolution

**P2 — resolved in source:** the earlier callback preceded `image_materialize::write`, so it forked the unrelated child before any real writer existed and could miss a parent-side writer regression. The earlier Linux RED was informative but used a different hook placement.

The final `child_write` now invokes `Hook::opened` immediately after successful raw `open`, before writing or closing. The child sends one fixed ready byte and waits for one release byte with raw `write`/`poll`/`read`. The parent `Hook::observe` calls the Rust observer only after receiving that ready byte. Thus the unrelated fork occurs while the actual executable writer is held in the materializer child, and the observer never opens a substitute image writer. The callback is not executed in the post-fork child: `opened` compares `getpid()` to the pre-fork parent PID before accessing the `RefCell` callback.

That parent-PID branch also makes the same hook meaningful for a parent-writer mutation: moving execution of `child_write` into the constructing parent causes the actual-open hook to call the observer there, while the real writer is open. No hook relocation is needed. The unrelated child then inherits that writer; the Linux exec assertion should reject it with ETXTBSY. Source inspection resolves the placement gap; retained execution of this exact mutation is still needed to demonstrate sensitivity.

The observer catches callback panic, sends release, and lets `write_inner` reap before resuming unwind or returning the observation error. The raw child handshake has a ten-second poll bound; parent read/write timeouts are five/one seconds. Single-byte handshake failure is a test failure rather than a production error-path change. The hook is `cfg(test)` and production opens remain child-owned.

## Production correctness

- `CString` conversion and signal-set setup occur before fork. Only a child can open the executable with write access. The parent publishes `HelperImage` after the known child terminates; the child does not fork again. Unrelated parent forks therefore cannot inherit this image writer.
- The post-fork routine uses inherited immutable path/byte storage, scalar/pointer operations, raw `open`, `write`, `fchmod`, `close`, platform errno access and `_exit`. It does not allocate, format, log, acquire Rust locks, unwind or run Rust destructors. Under the write syscall contract, positive progress is no larger than remaining length, so offset addition stays in bounds. Negative EINTR is retried; zero progress becomes failure.
- On child failure before explicit close, returning the small status reaches `_exit`; the OS closes the child's descriptors. On success, close precedes exit. Exclusive/no-follow creation prevents replacement of an existing path, and `image.rs` retains its parent-side RAII cleanup for incomplete staging. Exact embedded bytes and bundled-image verification remain intact.
- The private exit codes preserve existing error categories: 0 success; 2 ENOENT→NotFound; 3 EACCES/EPERM→PermissionDenied; all other failures→Io. Abnormal signal exit is Io. A raw errno diagnostic is intentionally not retained, but the public mapping does not lose a previously distinct category. There is no pipe whose inherited writer can delay EOF.
- Catchable signals are blocked before fork and the original parent-thread mask is restored before waiting. A started child is still reaped before a restoration error is returned. The fork error is captured before restoration can overwrite errno. Platform atfork callbacks remain an embedding constraint, now explicitly documented; the child routine's safety claim does not authorize arbitrary callback behavior.
- `reap` now keeps waiting through nonterminal traced statuses and only releases ownership on exited/signaled status. EINTR retries preserve the same positive child PID. The prior draft's stopped-child ownership gap is corrected. Linux documents traced stops even without `WUNTRACED`. [waitpid(2)](https://man7.org/linux/man-pages/man2/waitpid.2.html)
- A non-EINTR wait error is propagated, not proof of successful reaping. Under the normal documented call contract—valid positive child PID, options 0, and no competing reapers/auto-reap—EINVAL/invalid-PID cases are excluded and ECHILD means ownership is already unavailable. Do not add a speculative numeric kill after ECHILD. Backend construction checks host SIGCHLD policy before image creation; stability of that policy and exclusive child reaping are documented requirements.
- The new construction child adds a short-lived resource cost but no persistent per-session worker, channel or public limit. Filesystem work and waiting remain unbounded in wall-clock time, explicitly documented rather than concealed by retries/timeouts.

## Regression evidence and remaining verification

`docs/verification/image-fork/red/metadata.json` records Linux 6.8 x86_64, base `46dbec64384e4519e751086ef47feb5bac11024d`, targeted infrastructure unit test exit 101, and unchanged test-run sources. Its command log contains one failed test with errno 26. The test releases/reaps the unrelated child before inspecting the during-exec result and checks the same bytes execute after release; that makes the old red informative and avoids intentionally stranding the blocker.

The canonical fixture uses bounded pre-exec/child waits and guards, real helper execution with its established invalid-arguments exit 125, and byte equality. It releases/reaps the unrelated child before asserting either execution outcome and requires the same image to execute after release. The three new direct materializer tests check exact binary bytes and mode 0500 on success, missing-parent NotFound, existing-destination Io with original bytes preserved, and caller-thread signal-mask membership preserved for all three paths. Mask comparison avoids sigset padding. These tests do not exhaust permission, write, chmod, close, fork, or mask-restoration failure injection, and no such exhaustive coverage is claimed.

Execution results for the final four tests on macOS/Linux and the Linux parent-writer mutation are pending at this refresh. The earlier RED record remains historical evidence, not a substitute for that final mutation result.

## Exact reviewed hashes

| Source/evidence | SHA-256 |
| --- | --- |
| `crates/infrastructure/src/process/image_materialize.rs` | `2f3d02bcc547feabed0d65398abe34ccbd37381ae6c71696cfa75e0e02917604` |
| `crates/infrastructure/src/process/image_materialize_hook.rs` | `6b882c754d990d04891cc391c1a92473964d1283ad1b042d0049791794499ce7` |
| `crates/infrastructure/src/process/image.rs` | `505fc327cc1583cda99d165705fe7efadafb2eb1f1b2bea48480ee6ccd3a3ac6` |
| `crates/infrastructure/tests/fixtures/process_image_fork.rs` | `a41846b814763e33a2bc7bb9edc6315cd418586d9b6bb616073b4b3149a94f81` |
| `crates/infrastructure/tests/fixtures/process_image_materialize.rs` | `4d3afffec148af414720ce4d648f377cbe2665fb5ede161831f224adaca9412a` |
| `docs/verification/image-fork/red/command.log` | `047adc93caa6833321aa0546f7818c683c3847b56e5652e6a10642d58700a2ef` |
