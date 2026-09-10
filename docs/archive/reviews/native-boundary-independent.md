# Independent C boundary and coverage review

Reviewed 2026-09-08 on macOS arm64. Scope: boundary-contract.c,
boundary-shim.c, boundary-faults.h, run_boundary_contract.py, native/coverage.py,
and the retained native-coverage/boundary-repeat run. No production changes.
Current hashes and independent sanitizer log are in
`docs/verification/native-boundary-independent/`.

No new blocking correctness finding. The earlier unbounded history loop has
been replaced with an 8192-step bound and a required terminal error assertion;
exhausting the bound fails instead of hanging. The current candidate independently
passes `python3 scripts/native/tests/run_boundary_contract.py --sanitize` with
AddressSanitizer and UndefinedBehaviorSanitizer enabled for bridge/shim/test C.
The pinned Zig archive remains uninstrumented; this is not a sanitizer claim for
all instructions in that archive.

## Oracles and lifetime ownership

The test redirects only separately compiled bridge translation units, leaving
successful terminal/decoder/formatter operations in the real pinned library.
Fault hooks check their exact hit counts, so a returned error cannot pass merely
because some unrelated setup failed. Assertions distinguish generic errors,
capacity denial, unsupported compression, and the documented mode-query fallback.
Read outputs, cleared reply references, sticky allocator denial, retained decoder
ownership and constructor/restore cleanup are checked at their relevant boundaries.

The allocation shim counts successful malloc/calloc/aligned allocations and their
frees. Every terminal fixture uses checked_free and requires the count to return
to zero. Failed constructor/restore/formatter paths also assert zero live counts.
Native allocations using the supplied allocator flow through the shim. Aggregate
counts alone are not a pointer identity proof; the independent sanitizer run
adds invalid-free/use-after-free detection for instrumented code. Temporary input
and output buffers are separately allocated and freed by the fixture. Callback
fault state is reset before terminal teardown, and no callback is used after its
terminal lifetime. Synthetic empty callbacks and invalid style tags are explicitly
boundary cases, not assertions about normal engine output.

## Coverage provenance and denominator

The driver rejects a preexisting work directory, copies the source/native cache
and guardian image, uses a fresh target and profile directory, excludes build
profiles from its run-profile merge, records test exit codes and binary/source
hashes, and rejects missing contract executables. It selects exactly the four
owned bridge files and checks that all four are present in exported JSON.
Every recorded contract exits zero. The added --require-complete checks nonzero
integer totals and exact covered/count equality for lines, functions, regions
and LLVM branches; rounded percentages cannot make that gate pass.

All four current production C file hashes match the recorded boundary-repeat
source manifest. Its boundary fixture precedes the bounded-loop correction, and
its driver precedes --require-complete. Therefore the stored report is evidence
for those recorded test/driver versions; the independent sanitizer result is for
the current bounded fixture. A later full coverage invocation can bind the exact
current driver/test versions without changing the meaning of the recorded report.

Recorded combined counts are 237/237 lines, 22/22 functions, 334/334 regions and
191/191 LLVM branch entries. Independent remerge of the same retained profiles
with only run-boundary excluded gives 235/237 lines, 22/22 functions, 293/334
regions and 134/191 branches. The other tests, including allocator contracts,
remain in that comparison. This proves the synthetic faults materially contribute
to the 100% result; it must not be described as 100% real-engine behavior coverage.
No counters were fabricated and no uncovered bridge source was excluded. MC/DC
counts are zero and unmeasured. Rust, upstream engine implementation, other
platform builds and behavioral combinations remain outside this denominator.

The module docstring's real-native-tests description is narrower than the actual
optional combined collection; command help, per-test synthetic_faults metadata,
fixture comments and final acceptance prose must continue preserving the explicit
synthetic/real distinction. The coordinator owns full gate and final readiness.
