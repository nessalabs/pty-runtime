# Non-reflow resize must not leave an unpaired wide cell

Status: historical scoped red/green proof. The combined source has since changed
for the inactive viewport-pin defect in `terminal-viewport-pin.md`; these results
do not constitute a gate on the latest source.

## Red evidence and native convention

The extended deterministic corpus stopped at seed 132, operation 19, when the
uninterrupted reference terminal's checkpoint returned `EngineFailure`.
`work/terminal-corpus-review/dependency-row-wrap-seed132.log` preserves the exact
input/resize trace. The failure is separate from the earlier row-wrap growth
comparison failure and from low-history quota pressure.

`seed132-minimal.c` reduces it to a terminal with no retained history: create a
54-column alternate screen, CUP to one-based row 1/column 27, print `界`, checkpoint,
then shrink to 27 columns and checkpoint again. The original wide base at index 26
remains while its spacer tail at 27 is erased. Direct native encode changes from
success to `GHOSTTY_INVALID_VALUE` (-2); allocator denial remains false. The
result is retained in `seed132-minimal.log`.

The snapshot encoder correctly rejects a wide base without its following spacer
(`src/terminal/snapshot/grid.zig:486-491` in the pinned native source). The
non-reflow shrink path previously cleared only `[new_cols, old_cols)`, leaving
the base just before the cutoff. This is invalid live terminal state, so decoder
normalization or an oracle exception would conceal the defect.

Existing native conventions support clearing the whole clipped glyph:
`Screen.splitBoundary` clears both cells when a boundary cuts a wide pair;
`PageList resize reflow less cols to eliminate a wide char` expects a blank narrow
cell when a two-cell glyph cannot fit in a one-column terminal. The existing
`Page.clearCells` operation also releases grapheme, style and hyperlink state.

## Bounded fix

The existing non-reflow shrink clear range starts one cell earlier only when
the new final cell is a wide base. Both halves are therefore removed through
`clearCells`, while narrow content at the cutoff is preserved. The operation
adds no allocation, operating-system branch, serialization format or new API.
No history, native-memory, checkpoint or parser-continuation quota changes.

The fixed patch still changes exactly `snapshot/screen.zig` and `PageList.zig`.
The original pinned revision and archive SHA remain unchanged. Combined patch
identity: `a3a8c854621c53054cf8089da225258d9bd82907f78f90ec2cf5104b2c1208fa`.
Corrected PageList SHA: `a288b692c579a47411affc8d543e89d57690a587bc4e7eebe6d95c9010fd3aa7`.
The source verifier permits the previous reviewed PageList state only during
preparation, stages and verifies both corrected inputs before writing, and
requires the complete corrected state and recorded library digest for linking.
The runtime compatibility marker contains the new full patch identity.

## Behavioral tests and green evidence

Author logs below are under `work/terminal-corpus-review`, macOS arm64, pinned
Zig 0.16.0. The two new public tests failed before this correction with the same
immediate checkpoint `EngineFailure` (`dependency-wide-cutoff-before.log`).
They cover primary screen with wrap disabled and alternate screen, plain wide
text and styled/hyperlinked wide text with a combining mark. After shrink they
require immediate checkpoint/restore, a blank narrow cutoff cell, and matching
future `ZQ` input, cursor-position replies and visible cells. Checking checkpoint
before that future input prevents an overwrite from hiding the invalid cell.

The native regression checks cutoff widths 1 and 5, an adjacent row's narrow `N`,
and later growth without resurrection of the removed spacer tail.

- `python3 scripts/native/bootstrap.py --jobs 2`:
  `dependency-wide-cutoff-build.log`, actual baseline-CPU ReleaseFast rebuild.
- `cargo test --locked -p pty-runtime-infrastructure --features ghostty
  --test terminal_cursor_roundtrip --test terminal_contract --test terminal_bounds`:
  `dependency-wide-cutoff-after.log`, all 19 tests pass (8 cursor, 6 contract, 5 bounds).
- From the cached native source: `../zig-aarch64-macos-0.16.0/zig build test-lib-vt
  -Demit-xcframework=false -Doptimize=ReleaseSafe -Dcpu=baseline
  '-Dtest-filter=PageList resize (no reflow)' '-Dtest-filter=SCREEN restoration'
  '-Dtest-filter=saved cursor preserves valid pending wrap' -j2
  --global-cache-dir ../zig-global-cache --summary all`:
  `dependency-wide-cutoff-native-tests.log`, 125/125 tests and 46/46 steps pass.
- `python3 -m unittest discover -s scripts/tests -p test_native_source.py -v`:
  `dependency-wide-cutoff-verifier-tests.log`, 6 tests pass, including exact upgrade
  from the earlier growth-only patch and rejection of that state for linking.
- `python3 -m unittest discover -s experiments/tests -p test_native_build.py -v`:
  `dependency-wide-cutoff-cache-tests.log`, 2 recipe tests pass (simulated platforms).
- Pinned `zig fmt --check` on both corrected native files:
  `dependency-wide-cutoff-zig-format.log`, exit 0.
- `cargo run --locked -p pty-runtime-infrastructure --features ghostty
  --example terminal_corpus -- 132 1 64`:
  `dependency-wide-cutoff-seed132-after.log`, pass, 1912 ms, 40 feeds, 12 resizes,
  13 restores and 78 rejected malformed mutations.

Corpus execution passed seeds 133–170 and stopped at the distinct seed 171
inactive viewport-pin defect in
`dependency-wide-cutoff-corpus133-1000.log`; commands/platform/source digests are
in `dependency-wide-cutoff-corpus-source.json`. Previous seed 1–131 passing results
used the earlier patch identity and are not presented as a full corpus run on
this latest source. Previous full-gate/performance results are likewise
superseded for acceptance of the new source. Root owns the fresh full gate and
independent review after the active fixes/tests settle.
