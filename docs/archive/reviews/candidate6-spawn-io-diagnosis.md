# Candidate6 ordinary process spawn: bounded diagnosis

Read-only source/log review on 2026-09-08; no compilation, test execution or production edits. The coordinator owns Linux tracing. No native parser/checkpoint/crash investigation was undertaken.

## Established evidence

The retained candidate6 gate log fails at tests/raw_runtime.rs:58 during initial spawn, before attachment/detachment assertions. The failed leg is `cargo test --locked --no-default-features --features event-stream`; the same raw_runtime test passed in preceding all-features and no-default legs. Metadata records unchanged source based on 46dbec64384e4519e751086ef47feb5bac11024d, Linux x86_64 and rustc 1.97.1. This does not establish an event-stream defect or a resolved/transient failure.

The workload is the prebuilt CARGO_BIN_EXE_pty-runtime-fixture in echo mode. Four tests in this binary construct independent runtime/backends. Seven inspected source hashes below matched the failed-run source manifest. The earlier CI spawn diagnosis has an analogous undifferentiated Io symptom; neither log establishes a common errno or cause.

## Source-grounded mechanism to investigate

HelperImage creates a private random directory, writes the embedded image through a writable CLOEXEC descriptor, chmods it and returns. The local writer is dropped before successful constructor return, so no direct writer leak is visible. However, another backend's Command::spawn with pre_exec can fork while that writer exists. Its child can hold an inherited writable descriptor until exec; the first backend may close its parent writer and try executing the image during that interval. ETXTBSY is therefore a concrete candidate interleaving, **not an established diagnosis**. O_CLOEXEC does not itself mean close-at-fork. Per-backend spawner serialization does not coordinate image creation or forks across independent backends.

There are several other synchronous Io origins: PTY/descriptor setup, initial helper spawn including pre_exec, received helper StartFailed (workload launch or coordination), Execute queue failure, and reader thread/descriptor registration. Parent error conversion maps all ordinary IO kinds except NotFound/PermissionDenied to Io. Protocol faults and admission timeout have distinct results. The log alone cannot choose among these paths.

## Minimum next discriminator

The coordinator is tracing the existing failing test binary without changing source. A bounded successful traced run is only a passing observation; tracing changes timing and cannot exonerate any race. Retain the first failed trace/log and identify the failing syscall/errno before choosing a fix. Compare concurrent versus serial execution only as supporting evidence, not as a production workaround.

If tracing does not locate the boundary, add opt-in, error-only infrastructure diagnostics immediately before conversion: fixed stage, launch generation where available, ErrorKind and optional raw errno. Distinguish helper Command::spawn, PTY/descriptor setup, guardian StartFailed and Execute enqueue, and reader registration/thread creation. Use the transmitted StartFailed integer; for Linux ptsname_r use its return code rather than unrelated errno. Queue failures without an OS error must report errno unavailable. Capture parent spawn failure after Command::spawn returns; never allocate/log inside pre_exec. Do not expose errno through application/domain contracts or retain paths, argv, environment or payloads.

Only after evidence identifies the mechanism should a deterministic test force its relevant interleaving in a separate diagnostic checkout. For suspected inherited-image writers, use synchronization around staging/fork/exec to hold the inherited writer and observe actual exec failure; do not turn a held-writer experiment into proof of the original failure. For registration or helper StartFailed, follow that stage instead. No blind retries, test skipping or global launch serialization are justified by the current evidence.

## Inspected matching source SHA-256

| Source | SHA-256 |
| --- | --- |
| `crates/infrastructure/src/process/image.rs` | `89679e2176fb479bfaac400187b5dbc7d2e48894ce72e374bf70def2c295eed2` |
| `crates/infrastructure/src/process/spawn.rs` | `6253768af63b6b2c460980abe17ec91591805b6950738139dbd2cffee8c2f18e` |
| `crates/infrastructure/src/process/registration.rs` | `cf9c33f8f287023aabe170512a00bfc8d2d28af0d551771e1ecaf58738f2de57` |
| `crates/infrastructure/src/process/endpoints.rs` | `2908d62c210b9b17d961df56ded823701d7d4cdc504e8c747a8b7e50a2e5696f` |
| `crates/infrastructure/src/process/guardian.rs` | `1ec11d9bc7f22f5e34927561b964391471f875a844fb76c9705913b9bc2ab1a0` |
| `tests/raw_runtime.rs` | `c8d756f0656b192a59a3d1e320dcd235a7e0deaedb9b4cc049a93580c1d70702` |
| `tests/support/mod.rs` | `2dbe0cdef734120ca57ae553f8a78968b32807964e4ff3aacd3cfa4beb2bbe61` |

Retained failure log SHA-256: `9bff44f727db85c02a0f4fbf461d460c7d3d5cc376ee0ea21e49de0ef684eede`.
