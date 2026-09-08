# Own C bridge source coverage — 2026-09-08

The four own C bridge files measure **237/237 executable lines, 22/22 functions,
334/334 regions, and 191/191 branch sides: 100% in each metric** in the identified
macOS arm64 population. This combines real-engine contracts with explicitly
synthetic external-call faults. No production file, function, or error path was
excluded. It is not Rust, helper, upstream Ghostty, whole-project, or other-platform
coverage. MC/DC was not measured.

| Own C source | Lines | Functions | Regions | Branch sides |
| --- | --- | --- | --- | --- |
| `scripts/native/owner.c` | 92/92 | 12/12 | 125/125 | 70/70 |
| `scripts/native/checkpoint.c` | 44/44 | 4/4 | 62/62 | 36/36 |
| `scripts/native/view.c` | 69/69 | 4/4 | 108/108 | 73/73 |
| `scripts/native/verification.c` | 32/32 | 2/2 | 39/39 | 12/12 |

[Fresh-run report](boundary-repeat/coverage-report.stdout.txt),
[annotated lines and branches](boundary-repeat/coverage-annotated.stdout.txt),
[raw export](boundary-repeat/coverage-export.stdout.txt),
[commands and exit codes](boundary-repeat/commands.json),
[test binary hashes](boundary-repeat/test-outcomes.json),
[build identity](boundary-repeat/identity.json), and
[all copied source hashes](boundary-repeat/source-manifest.json) preserve the
complete claim. Every contract passed. A prior aggregate of the same real-engine
profiles and synthetic binary independently obtained the same result in
[boundary-final](boundary-final/coverage-report.stdout.txt).

## What was tested

Real-engine Rust contracts exercise the actual C bridge linked to the pinned Zig
archive: `terminal_contract` (6 tests, including complete 100,000-history-line
restoration), `terminal_bounds` (5), `terminal_live_restore` (3), and
`terminal_view_edges` (2). The latter asserts exact optional foreground/background/
cursor/palette override-and-reset semantics and a 65-codepoint grapheme's capacity
retry, budget refusal, and subsequent model usability. The allocator callback
contract checks aligned allocation/free, invalid alignment/budget refusal, and
resize/remap refusal preserving bytes/accounting. All are ordinary required tests.

The new [boundary contract](../../../scripts/native/tests/boundary-contract.c)
checks error translation and cleanup. A separately compiled
[shim](../../../scripts/native/tests/boundary-shim.c) redirects only calls from
own C objects; production sources are not textually included or modified. Native
handles, snapshots and successful calls come from the real pinned engine. Selected
external responses are deliberately injected:

* All eight configuration failures, owner/native allocation failure, failed
  aligned allocation, and formatter creation failure release every tracked own-C
  allocation. Tracking counts must reach zero after each complete owner teardown.
* Getter/cell/grapheme failures return errors. Unknown native color tags preserve
  the invalid sentinel (`255`) for the Rust boundary to reject. This C test does
  not itself establish Rust DTO rejection; that remains a separate Rust contract.
* Unsupported compression returns `-3`; native failures and processing-error
  responses return `-1`. A zero-byte reply preserves zero length and clears the
  borrowed reply buffer after feeding.
* Allocation denial established through the actual installed callback remains
  sticky across feed, resize, compression, checkpoint, and history calls; these
  calls report `-2`. This tests translation of already-established denial, not a
  claim that every operation independently induces memory exhaustion.
* Restore option/getter failures preserve ownership cleanup. The bounded
  verification writer rejects insufficient capacity; malformed decoder input and
  injected allocation/formatter failures fail cleanly.

These injected cases prove the C bridge response to a boundary result, not that
well-formed terminal input can induce those faults in Ghostty. They do not measure
or validate the engine's internal failure paths. The synthetic binary is explicitly
marked in `test-outcomes.json`. The ordinary real-engine-only result remains
[view-edges](view-edges/coverage-report.stdout.txt): 235/237 lines, 22/22 functions,
293/334 regions, and 134/191 branch sides.

The [runner](../../../scripts/native/tests/run_boundary_contract.py) verifies the
native source/build before linking and is included immediately after the allocator
contract in `scripts/gate.py`. The full root gate and specialist review remain
coordinator-owned acceptance requirements; a focused successful run is not a gate
or release-readiness claim.

## Instrumentation and source identity

Apple Clang 21 and matching Apple LLVM 21 tools use `-O0
-fprofile-instr-generate -fcoverage-mapping -fprofile-update=atomic`. Rust and the
Zig archive are uninstrumented. The [coverage driver](../../../scripts/native/coverage.py)
copies source/native inputs/helper image to a fresh work directory, verifies the
copied native build, and builds a private Cargo target. Unique per-process/module
profiles are collected only from runtime binaries; build-time empty profiles are
not merged. All six binaries are supplied to LLVM coverage. Atomic counters and
serialized native tests prevent the non-atomic counter artifacts seen initially.

All four measured production source hashes matched the main worktree:

* owner: `a12e77c84a5ada3c02a9516b2ddadc681c2d795f71a7d3dd614c3711bf3389b9`
* checkpoint: `0db1df90abc4d1d5bce7ca98a530a9513d3b6d8eb0ca96a9b9933ab0429ceba1`
* view: `1647edec00ad3d27cdc12785d9fbb53d4649ceddc4c1b5750e135cd659b33c11`
* verification: `2a01968543507d95e5c39dcd585ba277896c0f57ed8dfe94047c29d94523c185`

The coverage snapshot uses `work/coverage-baseline`, Ghostty base
`82232ecde55405559dec29c5466cb9e39938cb41`, patch SHA
`0a945af64ff9636971fe89b88d1aca95eb5867ae4e61397b1e8b1e92f5e0c67b`, and library SHA
`9e952e7d8af3d474226e49ceb7c8855f286e3f7a1968713802ec31eed2dc09cb`, built with
Zig 0.16.0 ReleaseFast/baseline CPU. That is the earlier cursor-patch baseline,
not the final corpus repair candidate. It isolates own-C measurement and does
not transfer behavioral proof between native patch versions.

Additionally, the boundary runner passed normally and with ASan/UBSan against the
then-current verified native candidate, library SHA
`f7b084a6cfe83b58803109b3329641a4b53c76c66b5cb053f41c427886eacea2`.
[Current-candidate commands/logs](boundary-current/commands.json) and
[its complete build/source identity](boundary-current/identity.json) identify it.
ASan/UBSan instrument only own C/shim/test accesses, not the Zig archive. This
focused pass does not qualify the candidate's unrelated corpus behavior.

## Retained attempts and limitations

* [baseline](baseline/coverage-report.stdout.txt): tests passed but non-atomic
  parallel profiling produced impossible underflowed counters (`18.4E`). Its
  branch/path counts must not be used for coverage claims.
* [atomic](atomic/coverage-report.stdout.txt): 233/237 lines, 22/22 functions,
  289/334 regions and 129/191 branch sides after the counter repair.
* [view-edges](view-edges/coverage-report.stdout.txt): adds meaningful public view
  contracts, with the real-engine-only result stated above.
* [boundary](boundary/coverage-report.stdout.txt): initial synthetic cases reached
  all lines/functions but left 9 regions and 14 branch sides unexecuted.
* [boundary-final](boundary-final/coverage-report.stdout.txt) and independent fresh
  [boundary-repeat](boundary-repeat/coverage-report.stdout.txt): all four metrics
  reach 100%, with the explicit synthetic scope above.

While extending the synthetic fixture, 100 history rows remained within the READY
page, so the intended later-history progress assertion failed without reaching its
injected getter. The corrected fixture writes 5,000 rows, as the existing live
restore contract does; the targeted fault is then required to fire exactly once.
This was a fixture correction, not evidence of a production defect. The original
[failure record](boundary-current/fixture-correction.txt) is retained.

Coverage does not establish memory safety, native abort containment, concurrent
correctness, supported-platform execution, performance, release lifecycle counts,
or the required 12-hour soak. The overall 100% own-code target still depends on
the Rust/helper/platform inventories and their unmet coverage.

## Reproduction

Use fresh destinations to prevent stale-profile merging:

```sh
python3 scripts/native/coverage.py \
  --source-root work/coverage-baseline \
  --guardian-image helpers/guardian/target/release/pty-runtime-guardian \
  --allocator-test scripts/native/tests/allocator-contract.c \
  --extra-test crates/infrastructure/tests/terminal_view_edges.rs \
  --boundary-tests-dir scripts/native/tests \
  --work work/native-coverage-repeat-002 \
  --evidence docs/verification/native-coverage/repeat-002
python3 scripts/native/tests/run_boundary_contract.py
python3 scripts/native/tests/run_boundary_contract.py --sanitize
```

For a final native candidate coverage run, select a frozen source root containing
its matching verifier/compatibility and native build. Profiles/binaries stay in
local work directories; durable exports, hashes, and logs are stored here.
