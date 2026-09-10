# CI 71c1d6f ordinary spawn failure diagnosis

## Finding

GitHub Actions run `34278967513`, Ubuntu 24.04 gate, failed during the first ordinary survivor spawn in `process_failure_isolation`. The evidence does **not** establish a sentinel-loss regression, a native load-path regression, or a transient runner failure. The current error conversion discards the information needed to distinguish several launch and registration failures.

This is a bounded source/log review. No build, test, load workload, Linux operation, or runtime source change was performed for this diagnosis while the full soak was running.

## Retained evidence

- Source baseline: `71c1d6f`; relevant current process implementation and isolation test matched that baseline when reviewed.
- Retained log: `work/ci-71c1d6f-failed.log`, SHA-256 `90fd06b964ee2a84d5c4977ddd99fd7b684eb526f67079b1c8242699920f3735`.
- Log lines 726–745: test binary starts at `21:11:31.9858882Z`; two tests run. Sentinel-loss test reports failure at `21:11:31.9898224Z`; guardian-loss test reports success at `21:11:32.0198086Z`.
- Panic: `crates/infrastructure/tests/process_failure_isolation.rs:37:10`, `called Result::unwrap() on an Err value: Io`.
- The successful dedicated Linux gate and macOS/MSRV checks reported by the coordinating agent are useful contrasting observations, but do not explain this failure or prove it transient.
- The coordinating agent subsequently reported that the failed-job rerun of the same run (attempt 2) succeeded on unchanged `71c1d6f` source; the earlier macOS/MSRV passes were retained. Both attempts are being preserved. This is evidence of a pass on rerun, not an explanation of the original `Io`, and does not establish a transient cause.

In `damaged_session`, backend construction completes at line 25. Line 37 unwraps the survivor's initial `/bin/sh` spawn. The victim spawn, parsing its process IDs, and intentional `SIGKILL` are all later. The failing invocation therefore never performs its own deliberate helper damage. Log completion timestamps do not establish the relative ordering of the other test's internal operations.

## Where `Io` can originate

The backend dispatches to `spawner`, which calls `spawn::launch`; a successful launch is passed to supervisor registration. Both launch and registration failures can be returned by the user's `spawn` call. `process/mod.rs:19` maps only `NotFound` and `PermissionDenied` separately; every other `std::io::Error` becomes `ProcessError::Io`.

| Stage | Relevant code | Information currently lost |
| --- | --- | --- |
| CWD and descriptor setup | `process/spawn.rs`: canonicalize/open directory, host clone, socket pairs, channel setup, `F_DUPFD_CLOEXEC` copies | Syscall/stage, raw errno |
| PTY setup | `process/endpoints.rs`: `posix_openpt`, `grantpt`, `unlockpt`, `ptsname_r`, slave open, terminal configuration, nonblocking setup | Syscall/stage, raw errno; `ptsname_r` returns an error code directly |
| Initial helper process | `process/spawn.rs:90`: `Command::spawn`, including `fchdir` and `dup2` in `pre_exec` | Raw spawn/pre-exec/exec error |
| Admission | `process/guardian.rs:93,231`: Execute enqueue failure or received `StartFailed` | Enqueue reason or transmitted errno |
| Workload start inside helper | `helpers/guardian/src/guardian.rs:22–35`, `workload.rs:20–57` | Sentinel coordination failure sends `ECANCELED`; workload descriptor clone, process-group/terminal setup, signal reset, or spawn failure sends its errno (or `EIO`) |
| Reader registration | `process/registration.rs`: host clone, wake pair, reader thread creation | Stage and raw errno, including a possible thread-creation resource failure |

A protocol fault maps to `Internal`, and the five-second admission deadline maps to `Timeout`; neither is the reported value. Capacity validation/admission failures also have separate variants. The very short log interval is consistent with an immediate failure, but cannot identify its stage.

## Source-based narrowing

Both tests create separate backend owners and private helper images. The repeated `SessionLifetime::new(801, 1)` is not evidence of a collision: the infrastructure spawn implementation ignores that argument and helper protocol generations use a process-wide atomic counter. The fixture does not globally mutate environment or signal handlers. There is no retained evidence that one test signals the other's helper.

The reader-memory instrumentation is reached in `io::reader` after reader-thread creation and successful helper admission. Its gauge update runs only when events provide diagnostics; this fixture uses the default events without diagnostics. Its scratch allocation does not return a synchronous `Err(Io)` to `spawn`. Thread creation can return `Io`, but that fallible operation predates the gauge change. This excludes a direct gauge-error return path; it does not establish the absence of all timing effects.

The bundled-image `O_NONBLOCK` verification change is not on this fixture's path: backend construction uses the embedded image, without a bundled-image argument. Backend image construction itself succeeded before the failing unwrap; execution of that image can still fail later.

## Hypotheses, not conclusions

1. **OS resource or interrupted setup operation.** Descriptor, process, thread, PTY, or other setup errors are all collapsed to `Io`. The log has no errno, resource limits, or contemporaneous resource measurements. Resource exhaustion and interruption must not be asserted from this symptom alone.
2. **Parallel image staging and process creation.** Each backend writes a private executable with a writable `O_CLOEXEC` descriptor, then closes it on return from `HelperImage::new`. There is no simple writer leak in that function. A specific concurrency mechanism to investigate, if initial helper spawn reports `ETXTBSY`, is another thread forking while the image writer is open: a child can retain an inherited writer until exec, potentially overlapping execution of that image after its parent writer closes. The source has parallel backend creation and a `pre_exec` spawn path, making this worth checking; the retained evidence does not demonstrate that interleaving or errno.
3. **Helper coordination or workload start.** A received `StartFailed` also becomes `Io`; it cannot currently be separated from host-side setup failure. The successful peer test does not eliminate this branch.

## Smallest next diagnostic and reproduction

After the coordinating agent releases the current soak/source freeze, add an opt-in, error-only infrastructure diagnostic that records a fixed stage label, error kind, and `raw_os_error()` immediately before conversion. Cover initial helper `Command::spawn` separately from PTY/descriptor setup, received `StartFailed` (its raw transmitted integer), Execute enqueue rejection, and reader registration/thread creation. For `ptsname_r`, record its returned error code instead of an unrelated `last_os_error`. Include a launch/generation identifier where available to correlate concurrent attempts. Do not log command arguments, environment, or PTY payload; do not add a domain error variant merely to diagnose this failure. Do not log or allocate inside `pre_exec`; record the returned spawn error in the parent.

First run a bounded Linux reproduction of the existing isolation test binary, retaining the first failure and stage/errno. Compare its default concurrent execution with `--test-threads=1`. A serial pass alone is not a fix or proof of a race. If concurrency correlates with failure, a small ordinary-spawn-only fixture with two concurrent backend constructors and survivor launches can isolate the setup from all intentional helper damage. Preserve the private-image staging path. If the reported stage is registration or helper `StartFailed`, follow that evidence instead of pursuing the image hypothesis.

Do not introduce blind retries, serialize the production path, skip the isolation test, or mark the run transient before locating the failing stage. This review proposes the next bounded experiment; it does not execute or authorize changes to the active soak.
