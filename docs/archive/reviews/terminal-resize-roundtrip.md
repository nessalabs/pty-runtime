# Terminal resize / snapshot contract diagnosis

Status: P1 snapshot correctness defect reproduced; bounded dependency fix
implemented and awaiting independent acceptance. The initial diagnosis below
was read-only. The implementation and author verification are recorded afterward.

## Evidence

Reviewed the repository instructions and coding standards, the native bridge,
the real terminal corpus, and Ghostty source pinned at
`82232ecde55405559dec29c5466cb9e39938cb41` under `work/experiment-cache`.

- `work/terminal-corpus-review/seed14.log`: after alternate-screen resizes from
  53 columns through 64 to 100, restoration changes cursor pending wrap from
  true to false at column 52. Cells and reported terminal modes otherwise match.
- `work/terminal-corpus-review/minimal-wrap.log`: the independently prepared
  minimal test enters alternate screen at 53 columns, writes exactly 53 ASCII
  characters, expands to 100 columns, and checkpoints/restores. Feeding `Z`
  afterward moves the original cursor to column 1, row 1, but the restored
  cursor to column 53, row 0. This demonstrates divergent future terminal
  behavior, beyond a projection-field mismatch.
- Ghostty `include/ghostty/vt/snapshot.h:25-30` promises complete terminal state
  and a READY prefix sufficient to render and resume, including unfinished
  parser input. No documented exception permits losing pending wrap on resize.
- `scripts/native/checkpoint.c:12,25-28` uses the existing native snapshot
  encoder and decoder. It does not synthesize cursor state.
- Ghostty `src/terminal/Terminal.zig:4099-4106` resizes alternate screen without
  reflow. `src/terminal/Screen.zig:2129` reloads the cursor; `cursorReload` at
  lines 876-919 updates coordinates and page references, retaining pending wrap.
  Saved cursor handling at lines 2160-2163 separately clears pending wrap and
  advances the saved cursor when it is away from the new final column.
- Ghostty `src/terminal/snapshot/screen.zig:414-415` explicitly clears restored
  active pending wrap unless the cursor is on the physical page's last column.
  Saved-cursor decode at line 827 uses an equivalent last-column condition.

## Contract judgment

The oracle is justified in rejecting this roundtrip: the exact same subsequent
print produces a different terminal state. Increasing history capacity does not
repair this condition. There is no evidence of OS, allocator-size, or process
transport dependence in the cursor-state transition.

There is an important reason not to conclude that all pending wrap away from
the final column is invalid. `Terminal.zig:1577-1579` sets pending wrap at the
effective right limit. `printWrap` at lines 1758-1798 explicitly handles wrapping
inside a right margin, marking soft-wrap metadata only at the full screen edge.
The snapshot normalization test at `snapshot/screen.zig:2056-2064` says such
positions would trip native printing assertions; that assertion exists in the
restricted `Screen.testWriteString` helper (lines 3744-3745), whereas actual
terminal printing permits the right-margin case. Therefore normalizing the
resize output alone is not a complete snapshot correction.

## Smallest portable approach

Keep the existing native API and the strict behavioral oracle. Correct this in
the dependency's snapshot decoder: preserve pending wrap for source coordinates
that are valid, including a cursor away from the full screen edge. Review both
active and saved cursor restoration; retain explicit handling of out-of-range
coordinates rather than conflating coordinate validation with last-column
normalization. This is a recommendation requiring native tests, not an applied
or accepted fix. If resize semantics are changed independently, specify and
test that behavior separately from checkpoint equivalence.

The reviewed C API exposes pending wrap through a terminal-data getter, but has
no matching setter or snapshot decoder option to retain it. Do not inject VT
cursor commands, edit opaque snapshots in Rust/C, weaken the oracle, or add a
platform-specific correction. Those approaches would duplicate terminal
semantics, affect parser/cursor state, or conceal the observed defect.

Required native regressions before acceptance: alternate-screen width expansion
with pending wrap followed by an ordinary character; right-margin pending wrap
without any resize followed by the same continuation on original/restored
terminals; saved cursor roundtrip and restore; shrinking and row-only resize;
existing malformed-coordinate normalization tests. The right-margin case is
source-backed here but has not yet been independently executed by this reviewer.

## Verification limits

Reviewed the root agent's actual minimal-test output, not a mock. This reviewer
did not rerun the failing case or the full gate and did not run performance
tests. The root review loop must retain its gate and performance evidence and
obtain independent rereview after any fix. No passed milestone or resolved
finding is claimed by this diagnosis.


## Bounded implementation and author verification

The parent agent subsequently authorized the single native source correction and
its existing build-path integration. `scripts/native/patches/README.md` records
all before/after/patch identities. The original archive checksum remains mandatory.
The patch preserves active and saved pending wrap for original in-range cursor
coordinates; clipped coordinates retain the previous last-column normalization.
No terminal resize semantics, native API surface, domain model, or operating
system behavior were changed by this subtask.

Author commands and raw evidence, macOS arm64 with pinned Zig 0.16.0:

- `python3 scripts/native/bootstrap.py --jobs 2`: actual ReleaseFast baseline-CPU
  native build completed successfully; `work/terminal-corpus-review/patched-native-build.log`.
- `python3 -m unittest discover -s scripts/tests -p test_native_source.py -v`:
  three verifier tests pass, including exact application/idempotency, tamper
  rejection, and patched-library identity; `dependency-source-verifier-tests.log`.
- `python3 -m unittest discover -s experiments/tests -p test_native_build.py -v`:
  two cache contract tests pass for simulated macOS/Linux recipe selection;
  `dependency-build-cache-tests.log`. These are orchestration tests, not Linux
  native execution.
- From the cached Ghostty source: `../zig-aarch64-macos-0.16.0/zig build test-lib-vt
  -Demit-xcframework=false -Doptimize=ReleaseSafe -Dcpu=baseline
  '-Dtest-filter=SCREEN restoration'
  '-Dtest-filter=saved cursor preserves valid pending wrap' -j2
  --global-cache-dir ../zig-global-cache --summary all`: 46/46 build steps and
  69/69 selected/dependency tests pass; `dependency-native-cursor-tests.log`.
- `cargo test --locked -p pty-runtime-infrastructure --features ghostty
  --test terminal_contract --test terminal_bounds --test terminal_cursor_roundtrip`:
  initial run passed all 15 tests, but that initial output path was subsequently
  reused by the root agent. The retained author rerun is
  `dependency-fix-contracts.log`: bounds 5/5 pass; contract 5/6 pass, with
  `compression_preserves_wide_combining_text_and_full_history` reporting
  `BudgetExceeded` at line 263 during checkpoint after compression. The cursor
  target was not reached in that rerun. Shared allocator edits occurred between
  these runs; causality is not established by this record. This failure was
  reported to the root agent and is not hidden by the initial passing result.

All abbreviated log paths above are under `work/terminal-corpus-review`.
The full gate and scoped performance record remain the root review loop's
responsibility; they were not run by this implementation subtask. The codec's
focused native results do not resolve unrelated shared-worktree failures or
constitute release/architecture acceptance. Independent root public cursor
regressions and rereview are required before the P1 finding is closed.


## Superseding row-wrap correction

A later corpus case exposed allocation-capacity-dependent non-reflow resize
behavior. The diagnosis and combined-patch verification are recorded in
`terminal-row-wrap-roundtrip.md`. The combined patch supersedes the cursor-only
patch identity and any full-source gate recorded before this second correction;
this document's cursor-only raw evidence remains historical evidence for that
specific fix, not acceptance of the newer combined source.
