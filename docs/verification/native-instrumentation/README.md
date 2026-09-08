# Available native instrumentation — 2026-09-08

Apple Clang 21.0.0 (`clang-2100.1.1.101`), macOS 26.6 build 25G72,
arm64, compiled the existing native fixtures with AddressSanitizer and
UndefinedBehaviorSanitizer. Both completed with exit 0 and empty sanitizer stderr.
`otool -L` confirms the ASan runtime is linked. This is bounded C-side evidence,
not a claim that the complete native engine was sanitizer-instrumented.

## Identity and reproduction

`execution.json` preserves each exact compiler/run command, working directory,
exit code, explicit runtime options, and SHA-256 identities of the fixture files,
included helpers, patched source, library, and focused bridge test. Individual
stdout/stderr files preserve raw results. `native-build.json` records the verified
native build. The worktree was based on `52ad04c3519616e6cedea9ac8707406970a40ed7`
and dirty; `source-status.stdout.txt` records this rather than presenting HEAD as
the identity of all tested files.

The native source verifier (`--built`) succeeded before these tests. Ghostty base
revision is `82232ecde55405559dec29c5466cb9e39938cb41`; the pending-wrap patch SHA-256
is `0a945af64ff9636971fe89b88d1aca95eb5867ae4e61397b1e8b1e92f5e0c67b` and the
linked static library SHA-256 is
`eb3d09bcd1984b9f30b184bea0e045fabc654b1a1e1b9fa62d4fc6aadc0eba09`.
It was built with Zig 0.16.0, `-Demit-lib-vt -Demit-xcframework=false
-Doptimize=ReleaseFast -Dcpu=baseline`.

The C sanitizer flags were `-O1 -g -fno-omit-frame-pointer
-fsanitize=address,undefined -fno-sanitize-recover=all -Wall -Wextra`.
The allocator test additionally used `-std=c11 -Werror`.
Both existing fixtures ran with `ASAN_OPTIONS=halt_on_error=1` and
`UBSAN_OPTIONS=halt_on_error=1:print_stacktrace=1`.

## Actual coverage

* `verify-roundtrip.c`: six existing ground/UTF-8/CSI/OSC/DCS/alternate-screen
  continuation cases with 10,000 varied input lines each. Every case checks
  immediate binary roundtrip, ten additional decode/encode cycles, resumed input,
  resize, replies, and rejection of a truncated and checksum-corrupted snapshot.
  Its allocator mode 2 uses mmap for larger allocations; accounting returns to zero.
* `native-memory.c 1 10000 1 1 8 0 1`: one terminal, 780,000 input bytes,
  compression enabled, 8 MiB history cap, malloc-backed custom allocator mode 1.
  It checks complete formatted-state/continuation restoration, reports 16 history
  pages and 9,456 restored history rows with zero skipped pages, and returns native
  tracked bytes to zero. This sanitized sample is not performance acceptance data.
* `scripts/native/tests/allocator-contract.c`: includes the actual production
  owner implementation and invokes its installed allocator callbacks. Values 0
  through 6 and 12 allocate, satisfy 1 through 64 and 4,096 byte alignment, and
  free back to zero. Invalid exponent 255, requested bytes above the budget,
  and alignment bytes above the budget return NULL, mark denial, and leave
  accounting at zero. The final run is `allocator-aligned-contract`, exit 0
  with empty sanitizer stderr. The pinned
  `src/lib/allocator.zig` passes `@intFromEnum(std.mem.Alignment)`; Zig's
  `lib/std/mem.zig` defines those as log2 byte alignment. The upstream C header's
  byte-alignment description disagrees with that actual ABI. The production
  implementation now uses `posix_memalign` above malloc's guaranteed alignment,
  with invalid-shift and configured-cap checks before allocation.

An earlier experiment rejected all alignments above 4. Its narrow callback test
passed, but the broader native compression contract subsequently showed that
valid engine operations need higher alignment. That proposed behavior was
superseded. Its `allocator-contract` logs and the comparison that restored the old
`>16` threshold (`allocator-before-run`, signal 6 at the denial assertion) are
preserved as historical experiment evidence, not final acceptance. The final
test instead requires correctly aligned allocations. `execution.json` distinguishes
the superseded source hashes from the current callback test and owner hashes.

After native bootstrap, run the focused callback regression portably with:

```sh
python3 scripts/native/tests/run_allocator_contract.py
python3 scripts/native/tests/run_allocator_contract.py --sanitize
```

The helper verifies the source and supports Linux's additional system link
libraries; Linux execution was not performed for this evidence. The fixture
roundtrip/lifecycle runs use the direct C API and do not test the production
bridge. The focused callback test covers only that bridge's allocator boundary.

## Limits and remaining evidence

The linked Zig engine is the normal ReleaseFast static archive. Compiling its
C caller with sanitizers does not instrument Zig memory accesses or all engine
undefined behavior. Intercepted malloc/free can expose some allocator misuse,
but mmap allocations do not receive ordinary ASan heap redzones. Accounting
assertions are not a general leak proof. LeakSanitizer and an engine-wide
instrumented Zig build were not run. Rust sanitizer instrumentation was not run
here. These cases are not arbitrary malformed-snapshot fuzzing, a long soak,
or a fault-isolation test: native assertions/aborts can still terminate the host
process. The red regression itself demonstrates process termination, not its
containment. No release milestone is asserted by these samples; repository gate
and independent specialist acceptance remain separate requirements.
