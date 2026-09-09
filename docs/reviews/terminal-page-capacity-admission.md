# PAGE decoding must reject a capacity a native page cannot address

Status: author targeted tests pass, seed 201 now passes, and the full macOS gate
is recorded under `docs/verification/page-capacity/`. Root owns independent DDD,
design, and correctness review, and the Linux gate. No release acceptance is
claimed here.

The corpus run that this correction unblocked then stopped at seed 509 on a
second, independent native defect, diagnosed in
`terminal-continuation-c1-introducer.md`. The complete seeds 1-1000 corpus is
recorded under `docs/verification/continuation-c1/corpus-1-1000/`, which is the
first run of that range covering both corrections. This defect is reported
upstream as Ghostty discussion 14185.

## Failure

`cargo run --locked -p pty-runtime-infrastructure --features ghostty --example
terminal_corpus -- 201 1 64` exited with SIGSEGV (-11) on macOS arm64. The
recorded native stack ended in `_platform_memmove` beneath
`terminal.snapshot.hyperlink.decodePageString`, reached from
`terminal.snapshot.page.decodePayloadBody`. The earlier bitmap allocator
correction did not resolve it; `docs/verification/resumed/native-seed201`
preserves that reconfirmation.

## Cause

The two saved snapshots in `work/terminal-corpus-review` are both 5,008 bytes and
differ only at offsets 1037-1039. Walking the record framing places the third
record, a PAGE, at offset 1009 with its payload at 1019, so the fixed 20-byte
PAGE header occupies 1019-1038 inclusive. The mutation therefore lands on the
last two bytes of the header's `string_capacity_bytes` field and the first byte
of the following hyperlink table entry's native ID. The advertised string
capacity changes from 2,048 to `0xffff0800`, that is 4,294,903,808 bytes.

`Header.pageCapacity` validated only that the dimensions were nonzero and copied
every capacity hint through unchanged. `PageList` page creation then computed
`Page.layout` for that capacity and only asserted the result:

    assert(layout.total_size <= size.max_page_size);

A native page addresses its members with `size.OffsetInt`, a `u32`, so that
assertion is the layout's precondition rather than a diagnostic. `ReleaseFast`
drops it, which permits the surrounding 64-bit layout arithmetic to be narrowed
to 32 bits. The page was then allocated for the truncated size while its string
allocator kept the advertised capacity.

Debugger state at the fault confirms the mechanism directly. The page reports
`capacity.string_bytes = 4294903808` with `memory.len = 16793600`, while
`string_alloc` holds `bitmap_count = 2097121` and a `chunks` offset of
16,855,200, which is 61,600 bytes past the end of that memory. The first
hyperlink URI allocation therefore returned a pointer outside the mapping and
`readSliceAll` wrote 20 bytes through it.

The same header reaches this code from live-screen PAGE decoding and from
history PAGE decoding, and every capacity field can exceed the limit on its own:
the string and grapheme hints are `u32`, and maximum `u16` columns and rows
alone require about 34 GB of cells.

## Bounded correction

`Header.pageCapacity` now builds the capacity, measures the completed
`Page.layout(capacity).total_size`, and returns the new
`CapacityError.CapacityTooLarge` when it exceeds `size.max_page_size`. The check
runs inside `Decoder.init`, before any page is allocated, so both PAGE decoding
paths reject the record and the whole restore fails cleanly. `CapacityError` was
already part of `PayloadDecodeError`, so no error set was enumerated by hand, and
the C boundary maps the new value to `invalid_value` through its existing
`else` arm. Measuring the assembled layout rather than each field keeps one
admission rule for capacities that only exceed the limit in combination.

`Page.layout` itself performs no allocation and asserts nothing, so evaluating it
for an untrusted capacity is safe. The correction adds no allocation, quota,
parser, schema or operating-system path.

The fixed patch now also changes `src/terminal/snapshot/page.zig` against the
original pinned Ghostty revision and archive.

Patch SHA-256: `2f94f382946d59e97b6d03b625092964e8cb62662acff75e9aec09866d4b71e7`.
Corrected `snapshot/page.zig` SHA-256:
`3d10248e58c4b4019ff463e6bcc829fe808e82b9c88a448433d4cc7a178f5052`.
The verifier, build stamp, and runtime compatibility marker require this exact
combined identity, so checkpoints written by the previous marker are rejected
rather than decoded.

## Verification

macOS arm64, pinned Zig 0.16.0, baseline CPU configuration.

- Native red/green: with the new bound removed and nothing else changed,
  `zig build test-lib-vt -Dtest-filter="capacity fits a native page"` fails,
  aborting in `decode validates that the capacity fits a native page`. With the
  bound restored the same filter passes 65/65 tests over 46/46 build steps, and
  the unfiltered `zig build test-lib-vt` exits 0. Logs are preserved under
  `docs/verification/page-capacity/`.
- The new native test covers the observed `0xffff0800` string capacity, a
  maximum `u32` grapheme capacity, and maximum `u16` dimensions, and asserts
  that an ordinary capacity is still admitted.
- `zig fmt --check` on the corrected source: exit 0.
- Seed 201 now passes with 114 malformed mutations rejected and no accepted
  mutation, recorded in `docs/verification/page-capacity/seed201/`.
- `python3 scripts/gate.py` passed with an unchanged source inventory, recorded
  in `docs/verification/page-capacity/macos-gate/`.
- Corpus seeds 1-1000 is recorded against the later combined source in
  `docs/verification/continuation-c1/corpus-1-1000/`, since a second defect
  blocked the range at seed 509 under this correction alone.

This is scoped evidence for one decoder admission defect. It is not a complete
native corpus beyond the recorded range, and no release acceptance follows.
