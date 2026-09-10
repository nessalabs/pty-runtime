# Helper image writer ownership: design options

## Recommendation

Prefer a small **portable materializer child** that opens, writes and closes the helper image only after fork, and whose parent waits for its exit before publishing `HelperImage`. This removes the writable image descriptor from the multithreaded host's descriptor table, covering unrelated host forks without a process-wide coordination assumption. Preserve the existing private directory, exact bundled-image verification, embedded bytes, executable path, public API and per-session admission bounds.

This is a source/design review, not an implementation or execution result. No builds, workloads, production edits or native parser/crash investigation were performed. The retained untraced diagnostic `docs/verification/candidate6-spawn-untraced/37.log` reports `initial_guardian`, `ExecutableFileBusy`, errno 26. That establishes the failing launch boundary/OS error. It does not retroactively prove every earlier generic `Io` had the same cause.

Reviewed SHA-256: diagnostic `6d45492b9712172f727a14bca49b30f1c96ae644fea7b192adefca754e7dec27`; `image.rs` `709b04f34482da2b823ad646d262688bd8253376e4be6841157c59cb5fe53c78`; `spawn.rs` `6253768af63b6b2c460980abe17ec91591805b6950738139dbd2cffee8c2f18e`.

## Why current staging permits the failure

`image.rs` opens the future executable for writing in the host, writes embedded bytes, chmods and closes the file on function return. `spawn.rs` uses a `pre_exec` mapping before executing that image. An unrelated concurrent fork can inherit the writer before the parent closes it. Renaming or chmodding the inode does not close that inherited descriptor. Linux documents `ETXTBSY` when an executable is open for writing. This is the source-visible mechanism consistent with the newly measured error. [execve(2)](https://man7.org/linux/man-pages/man2/execve.2.html)

## Portable materializer invariants

1. In the parent, verify the optional bundled image as today; create the private directory; precompute the NUL-terminated executable path, image pointer/length, mode, and a close-on-exec status channel. Allocate all Rust-owned state before fork. The parent never opens the executable with write access.
2. Fork a single short-lived child. Its routine only opens the new file with exclusive/no-follow ownership, writes the fixed bounded image using an EINTR-aware short-write loop, applies executable permissions, closes its writer, emits one fixed small status record, and calls `_exit`. No allocator, `std::fs`, formatting, logging, locks, unwinding, user callback, destructor path or second fork belongs in that child. The selected filesystem/FD operations and `_exit` are async-signal-safe; the design does not justify arbitrary Rust execution after a multithreaded fork. [signal-safety(7)](https://man7.org/linux/man-pages/man7/signal-safety.7.html)
3. Block catchable signals on the constructing thread before fork and restore the parent's mask promptly afterward; keep them blocked in the child until `_exit` so inherited application signal handlers do not run during materialization. Treat inherited atfork callbacks as a separate embedding/fork contract; a private mutex does not control them. Use the supported libc fork interface, not an improvised raw fork that bypasses runtime requirements.
4. The parent owns and reaps exactly that child, handles interrupted waits, and accepts success only after successful exit plus a valid status record. Waiting for this child establishes that its sole image writer is gone before `HelperImage` is returned. Preserve setup failure errno through existing conversion; abnormal exit or malformed/missing status is an explicit failure. Parent-side RAII removes incomplete filesystem artifacts on every error.
5. Do **not** make success or failure detection depend on status-pipe EOF: unrelated forks can inherit the pipe writer even though they cannot inherit the image writer. One robust order is wait for the known child, then read its fixed small record nonblocking. The record fits in the initially empty pipe, so the child does not require concurrent draining; a missing record cannot leave the parent waiting for another process to close an inherited writer. [pipe(2)](https://man7.org/linux/man-pages/man2/pipe.2.html)

This adds one transient backend-construction child and its status descriptors, with no persistent extra process or per-session descriptor. Construction already performs blocking filesystem work; a filesystem stall is still a construction limitation, not something a blind timeout/retry solves. Keep host child-reaping ownership explicit. The result preserves exact signed Mach-O bytes and the existing macOS path execution strategy. It prevents accidental inherited-writer races; it is not a new defense against malicious same-UID code deliberately reopening private files.

## Linux sealed memfd alternative

An executable memfd can hold the bounded embedded image and release it when references disappear. Populate it once, then require `F_SEAL_WRITE | F_SEAL_GROW | F_SEAL_SHRINK | F_SEAL_SEAL` before publishing the image. Seals are inode-wide, including inherited descriptors; full write sealing is stronger than `F_SEAL_FUTURE_WRITE`, which permits preexisting writable mappings. A failed seal operation must fail construction rather than silently expose mutable bytes. [memfd_create(2)](https://www.man7.org/linux/man-pages/man2/memfd_create.2.html), [file seals](https://www.man7.org/linux/man-pages/man2/F_GET_SEALS.2const.html)

For this codebase, a minimal Linux launch would retain one owned close-on-exec FD and use `/proc/self/fd/N` with the existing `Command::spawn` error pipe. **Move that FD above the fixed remap range before publishing its path**: current `pre_exec` overwrites descriptors 3–6, so an arbitrary low memfd could be clobbered. `F_DUPFD_CLOEXEC` with minimum 16 matches current launch conventions. Keep the owner alive for every launch and let exec close the inherited FD; the helper is an ELF binary, not a shebang script. Alternatively `execveat(AT_EMPTY_PATH)` can avoid proc path lookup, but would require more launch/argv/environment integration than this bounded repair. [execveat(2)](https://man7.org/linux/man-pages/man2/execveat.2.html)

Use explicit executable intent on supporting kernels. Kernel memfd policy can default unspecified files to non-executable or reject executable memfds entirely. A narrowly identified unsupported-flag probe on older kernels is different from retrying failed execution; policy denial must not trigger an unsafe filesystem fallback or policy change. Optional executable-mode sealing also needs feature handling. [Kernel memfd execution policy](https://www.kernel.org/doc/html/latest/userspace-api/mfd_noexec.html)

Memfd is a strong Linux-only image representation, but adds policy compatibility, proc-FD/remap handling, one retained FD/image allocation per backend, and a separate macOS implementation. It is therefore not the smallest cross-platform repair here. It remains a reasonable future deliberate Linux storage choice, with its resource delta reported.

## Rejected shortcuts and validation target

- A library-only image/spawn mutex covers only cooperating forks, so it does not establish the required ownership invariant.
- Blind ETXTBSY retries make success depend on another process eventually releasing a writer; serializing tests hides rather than removes the mechanism.
- chmod, close in the parent, rename, unlink, or copying to a second parent-written inode does not eliminate descriptor inheritance of the executed inode.
- A separately installed immutable helper avoids runtime writing, but introduces a deployment/source-of-truth contract beyond the existing embedded-helper API.

The deterministic red test should force an unrelated host child to survive across publication while inspecting/holding inherited descriptors, then demonstrate that the executable launches without waiting for that unrelated child to exit. For materialization, separately verify setup failure, child failure/status handling and cleanup/reaping without waiting for channel EOF. Keep tests focused on helper-image ownership and ordinary Rust/OS process launch; these designs do not concern the deferred native parser issue.
