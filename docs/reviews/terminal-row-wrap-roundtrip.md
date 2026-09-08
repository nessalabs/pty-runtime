# Non-reflow resize and snapshot row-wrap parity

Status: native defect reproduced; bounded fix implemented, awaiting independent
review and a fresh full-source gate. No quota or corpus comparison was changed.

## Diagnosis

`work/terminal-corpus-review/seed18.log` fails the full formatter comparison before
restore at operation 56: 6045 versus 6018 bytes. The difference is 29 spaces versus
CRLF between a prior `A` and the following row's content. Equal active views do
not cover row wrap metadata.

A read-only C replay of the recorded operations queried live rows through
`ghostty_terminal_grid_ref`, `ghostty_grid_ref_row`, and `ghostty_row_get`.
`seed18-row-flags.log` locates the first divergence at operation 52, width 59 to
88, following a restore at operation 50. The original row 6 has wrap=true and
row 7 has wrap-continuation=true; the restored terminal clears both. This occurs
before the subsequent random input and before the formatter is involved.

The small ASCII reproduction needs no retained history or malformed input:

1. Create a 100-column, 12-row terminal and enter alternate screen.
2. Resize to 59 columns, position the cursor at row 1 column 59, print `ALT`.
3. Snapshot and restore one copy; both have the same logical size and state.
4. Resize both copies to 88 columns. Original and restored live row metadata differ.

`seed18-minimal.c` and `seed18-minimal-alt.log` retain the independent C fixture.
A primary-screen variant disables wrap mode only for the non-reflow resizes,
then reenables wrap and resizes to 40 columns. The original cursor becomes
(column 10,row 2), while the restored cursor becomes (column 2,row 2), zero-based.
`seed18-minimal-primary.log` records this observable continuation divergence.
This is not a noncanonical formatter layout or the earlier low-history quota case.

## Native cause and smallest correction

All source references below are against pinned Ghostty
`82232ecde55405559dec29c5466cb9e39938cb41`, before this second correction.

- `src/terminal/PageList.zig:2930-2943`: non-reflow column growth with sufficient
  existing page capacity merely changes `page.size.cols`. Existing row wrap
  flags survive.
- The same function's replacement-page branches at lines 3012-3016 and
  3075-3079 call `cloneRowFrom` into wider pages.
- `src/terminal/page.zig:893-896`: copying fewer cells than the destination width
  retains the destination's row wrap flags. For these new rows both are false.
- `src/terminal/snapshot/page.zig:542-578` serializes logical columns/rows and
  reconstructs page capacity using those dimensions. Spare physical column
  capacity need not survive the snapshot; losing it selects the replacement
  path on the later resize.

The defect is native resize behavior depending on spare allocation capacity.
It is exposed by snapshots but requires no new serialization fields or API.
The correction clears wrap and wrap-continuation in the successful in-place
column-growth path, matching the existing replacement-page behavior. The
existing spacer-head regression already expects cleared wrap after growth.
The preliminary spacer-head scan remains unchanged; flags are cleared only
once the fast path is known to succeed, preserving its error behavior.

The fixed reviewed patch now changes exactly two native inputs. The original
archive SHA remains mandatory. `scripts/native/patches/README.md` records all
original/corrected hashes and combined patch identity
`d3cee6c6548ab641d4f528a5ce6280424a1caece5f67a4d48074f0c48f1b0af4`.
The source preparation verifies both temporary patched results before updating
source, accepts an existing cursor-only state for preparation, and requires the
complete corrected state for linking. Build stamps, CI cache keys and the runtime
compatibility marker cover the combined identity.

## Verification and superseded evidence

Author logs are under `work/terminal-corpus-review` on macOS arm64, Zig 0.16.0.
The public tests added to `terminal_cursor_roundtrip.rs` both fail before this
fix (`dependency-row-wrap-before.log`). The primary test compares subsequent
input's cursor-position reply and full grid, with explicit `LTZ` cell assertions;
the alternate test compares full canonical snapshot state after subsequent input.

- Native rebuild: `python3 scripts/native/bootstrap.py --jobs 2`,
  `dependency-row-wrap-build.log`.
- Public native contracts: `cargo test --locked -p pty-runtime-infrastructure
  --features ghostty --test terminal_cursor_roundtrip --test terminal_contract
  --test terminal_bounds`, `dependency-row-wrap-after.log`: all 17 tests pass.
- Source verification: `python3 -m unittest discover -s scripts/tests
  -p test_native_source.py -v`, `dependency-row-wrap-verifier-tests.log`: five
  tests pass, including mixed prior-patch upgrade and no publication when a
  second patched result fails its digest check.
- Build cache contract: `python3 -m unittest discover -s experiments/tests
  -p test_native_build.py -v`, `dependency-row-wrap-build-cache-tests.log`: two
  simulated platform recipe tests pass, not native Linux execution.

- From the cached Ghostty directory: `../zig-aarch64-macos-0.16.0/zig build
  test-lib-vt -Demit-xcframework=false -Doptimize=ReleaseSafe -Dcpu=baseline
  '-Dtest-filter=PageList resize (no reflow)' '-Dtest-filter=SCREEN restoration'
  '-Dtest-filter=saved cursor preserves valid pending wrap' -j2
  --global-cache-dir ../zig-global-cache --summary all`,
  `dependency-row-wrap-native-tests.log`: 46/46 steps and 123/123 tests pass.
- `cargo run --locked -p pty-runtime-infrastructure --features ghostty
  --example terminal_corpus -- 18 1 64`, `dependency-row-wrap-seed18-after.log`:
  pass, 64 operations, 21 restores, 126 rejected malformed inputs, 2655 ms.

The pre-second-fix macOS full gate and independently dispatched Linux
pre-fix gate do not establish acceptance of this new source. A fresh full gate,
scoped performance record, and independent rereview remain required. No full
release milestone is claimed here.


## Broader deterministic corpus limitation

The unchanged corpus passes seeds 1–100, 64 operations each:
`dependency-row-wrap-corpus100.log` reports 218629 ms, 3176 feeds, 1611 resizes,
1713 restores, and 10278 mutation trials (10253 rejected, 25 valid accepted).
The follow-up command uses seeds 101–1000 to target 1000 distinct seeds without
repeating the first 100. It stops at seed 132: the uninterrupted reference
terminal's checkpoint returns `EngineFailure` at runner line 83. Seeds 101–131
completed; the larger run is not a pass and no 1000-seed claim is supported.
`dependency-row-wrap-corpus101-1000.log` retains the failure and
`dependency-row-wrap-corpus-source.json` records commands, platform, and source
hashes. This new failure needs its own diagnosis; it does not justify lowering
history quotas, changing the oracle, or claiming the finite corpus is exhaustive.


The seed 132 trace was subsequently reduced without production changes:
create a 54-column alternate screen, place `界` at row 1 column 27 (one-based),
checkpoint successfully, then resize to 27 columns. The wide base remains at the
new final column while its spacer tail has been erased. Direct native
`snapshot_encode_buf` changes from success to `GHOSTTY_INVALID_VALUE` (-2), with
allocator denial false. `seed132-minimal.c` and `seed132-minimal.log` retain this
small reproduction, independent of history and random input.

The non-reflow shrink branch in pinned `PageList.zig` clears only cells starting
at the new column count; it does not include a wide base immediately before that
boundary. `snapshot/grid.zig:486-491` rejects this unpaired wide cell correctly.
This is a new native clipping defect, not evidence that the stricter snapshot
oracle or quota should change. Its fix and acceptance are outside the completed
row-wrap-growth correction recorded above.


The subsequently authorized wide-cutoff fix and its separate red/green proof are
recorded in `terminal-wide-cutoff.md`. That correction supersedes this document's
combined patch identity for current-source acceptance; the raw growth-fix traces
and pre-wide-cutoff corpus results remain historical scoped evidence.
