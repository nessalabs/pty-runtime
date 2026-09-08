# Inactive viewport pins must not change resize trimming

Status: author targeted tests pass. Root owns independent review and the fresh
full gate. Corpus continuation stopped with exit 139 at seed 201; this is scoped
evidence, and the wider corpus is not complete.

## Failure and cause

Seed 171 first showed a visible divergence during a row shrink after restore.
The saved parser continuation was initially suspect, but the raw native replay
exported identical continuation bytes after every operation. The reduced native
case creates a 10×3 terminal, prints 20 history lines, restores one copy, grows
to 10×6, homes and clears the display, writes `hello`, then shrinks to 10×2.
The uninterrupted terminal keeps `hello`; the restored terminal pushes it into
history and shows a blank active screen. Evidence under
`work/terminal-corpus-review` includes `seed171-minimal.c`, `seed171-minimal.log`,
`seed171-continuation.log`, and `seed171-split-totals.log`.

`PageList.viewport_pin` remains registered among tracked pins even when viewport
mode is `.active` or `.top`, where its position has no display meaning. Native
creation leaves it at the first history row, while snapshot building places it
at the active top. Later `trimTrailingBlankRows` respects all tracked pins and
can stop at that incidental internal location. The full seed trimmed 15 rows
in the uninterrupted terminal and zero in the restored copy. This is shared
native resize behavior, independent of the wrapper allocator and platform.

## Bounded correction

Before trimming, `.active` and `.top` modes move the unused internal viewport
pin to the first page at row zero and invalidate its cached row offset. `.pin`
mode is preserved, as are all external tracked pins. The code performs no
allocation and changes no quota, parser state, schema or operating-system path.
The existing fixed patch still changes only `snapshot/screen.zig` and
`PageList.zig` against the original pinned Ghostty revision and archive.

Patch SHA-256: `c50f296f19e488833dc1c57a16a3f2c631546bb2e1070343d33763b1b2e3c1f0`.
PageList SHA-256: `a7703d31bfc95c68446e466ba3cf5cc329bed405e3527fe5503d344547e50462`.
The verifier, build stamp, and runtime compatibility marker require this exact
combined identity. Previous reviewed states are accepted only for preparation
and upgraded from verified original archive contents.

## Verification

Logs below are in `work/terminal-corpus-review`, on macOS arm64 with pinned
Zig 0.16.0 and baseline CPU configuration. The public test
`restored_inactive_viewport_pin_does_not_push_live_text_into_history_on_shrink`
failed before the patch (`dependency-viewport-pin-before.log`). It now requires
matching future `Z` input and cursor reply, visible `helloZ`, cursor `(6,0)`, and
canonical full state. Native tests cover `.active` and `.top`, an external pin
that must continue retaining a blank row, and a real pinned history viewport.

- `python3 scripts/native/bootstrap.py --jobs 2`: actual ReleaseFast rebuild,
  `dependency-viewport-pin-build2.log`, exit 0. The preceding build log preserves
  a test-local shadowing compile error corrected before the passing build.
- `cargo test --locked -p pty-runtime-infrastructure --features ghostty
  --test terminal_cursor_roundtrip --test terminal_contract --test terminal_bounds`:
  `dependency-viewport-pin-after.log`, 20/20 tests pass.
- Native `zig build test-lib-vt -Demit-xcframework=false -Doptimize=ReleaseSafe
  -Dcpu=baseline '-Dtest-filter=PageList resize (no reflow)'
  '-Dtest-filter=SCREEN restoration'
  '-Dtest-filter=saved cursor preserves valid pending wrap' -j2
  --global-cache-dir ../zig-global-cache --summary all`:
  `dependency-viewport-pin-native-tests.log`, 129/129 tests, 46/46 build steps pass.
- Source-verifier tests: `dependency-viewport-pin-verifier-tests.log`, 6 pass;
  build-cache tests: `dependency-viewport-pin-cache-tests.log`, 2 pass.
- Pinned `zig fmt --check` on both corrected native sources:
  `dependency-viewport-pin-zig-format.log`, exit 0.
- `cargo run --locked -p pty-runtime-infrastructure --features ghostty
  --example terminal_corpus -- 171 1 64`:
  `dependency-viewport-pin-seed171-after.log`, pass in 2158 ms, 35 feeds,
  13 resizes, 17 restores, 102 malformed mutations rejected.

The seeds 172–1000 batch ran under this identity in
`dependency-viewport-pin-corpus172-1000.log`; exact commands and source digests
are recorded in `dependency-viewport-pin-corpus-source.json`. It completed seeds
172–200 and exited 139 while seed 201 was active, with no aggregate PASS. The
exact process result is retained in
`dependency-viewport-pin-corpus172-1000-result.json`. Seed 201 alone also exits
139 (`dependency-viewport-pin-seed201.log`), so this is an unresolved reproducible
failure on the candidate. No source changes were made during root validation.
Earlier corpus
batches used earlier patch identities, so they are not a complete corpus run
on this source. No exhaustive coverage or release acceptance is claimed here.
