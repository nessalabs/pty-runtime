# Native cache portability investigation

Date: 2026-09-08. Investigated GitHub Runtime gate run 34211241576, head `06508816021b17f325cb35dce626bef56bc15e20`. The original failure is retained as a failure; no rerun was used to replace it with a success claim.

## Evidence and confidence

The Ubuntu stable job restored `native-Linux-X64-393bc6215c5dc43ffb8fa39aabd9d18b29254c6e0203dd20418519f99b00eeb3` (09:40:11 cache hit; restored 09:40:24). Bootstrap ran at 09:40:30 and did not print a native rebuild. `terminal_bounds` passed five tests, then `terminal_contract` exited with signal 4/SIGILL at 09:41:00 before reporting a completed test. The same run's uncached Ubuntu MSRV job explicitly rebuilt libghostty at 09:40:29 and passed all six terminal_contract tests at 09:44:53. macOS stable also passed.

A concrete cache portability defect is confirmed in source. `experiments/run.py:build` previously invoked `zig build` without `-Dcpu` or `-Dtarget`, but keyed its native stamp only by dependency pins, OS/architecture, libc and optimization flags. The GitHub native cache key likewise omitted CPU features and the build recipe. Pinned Ghostty `src/build/Config.zig:90` uses `standardTargetOptions` on Linux; Zig 0.16's `lib/std/zig/system.zig:374-380` resolves the omitted architecture/CPU query with `detectNativeCpuAndFeatures`. Thus a library compiled with one runner's CPU features was reusable on a different CPU sharing the same Linux/x86_64 cache key. Ghostty separately chooses a generic macOS target, explaining why the same issue is particularly relevant to Linux.

This strongly supports cached unsupported ISA as the cause of this observed failure. It is not proof of the faulting opcode: the job did not retain a fault PC, disassembly or core dump. SIGILL alone cannot distinguish an unsupported instruction from an explicit Zig/compiler trap. The successful uncached build is supporting evidence, not a controlled same-CPU/same-Rust comparison. The exact old native artifact was not recovered for disassembly.

## Fix

The actual build driver is `experiments/run.py` (there was no `experiments/build.py`). Native builds now explicitly pass `-Dcpu=baseline`. Their stamp includes the full native build option list and the build-driver SHA-256, so legacy stamps, CPU-policy changes and recipe changes force a rebuild. `.github/workflows/runtime.yml` uses a new `native-baseline-v2` key hashing dependencies, driver and bootstrap. This prevents reuse of the old host-specific archive and makes later recipe changes visible to the immutable action cache.

`scripts/native/build.rs` was not edited; its guardian build-hook owner was notified. The equivalent experiment-workflow key was flagged to root for coordination. The changed stamp itself rejects old experiment archives even when that workflow restores an older cache, so the remaining key update avoids repeated rebuild cost rather than enabling unsafe reuse.

## Validation

`python3 -m unittest discover -s experiments/tests -p test_native_build.py -v`: two tests pass. They run the actual build-driver path with mocked external compilation/download, assert an explicit baseline CPU command for Linux and macOS, verify warm-cache reuse, and prove that changed CPU options, driver identity and legacy stamps each trigger rebuilding. These are command/cache contract tests, not claims of executing native code on Linux.

`python3 scripts/native/bootstrap.py`: actual local macOS ARM64 baseline rebuild passed. Native build output was captured at `/tmp/pty-native-baseline-build.log` and the generated experiment-build.json records the new options and driver identity. `cargo test -p pty-runtime-infrastructure --features ghostty --test terminal_contract`: all six tests passed against the rebuilt library. Root owns final Linux/CI rerun evidence; no claim of repaired GitHub Ubuntu execution is made before that run passes.

If SIGILL recurs after a verified baseline build, retain the binary and faulting PC/core, run terminal_contract with one test thread and individual test filters, and inspect whether the failing instruction is an ISA requirement or an explicit trap. Do not conclude that another cache-key change fixes an unlocalized native fault.
