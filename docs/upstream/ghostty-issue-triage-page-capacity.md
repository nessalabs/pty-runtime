# DRAFT — Ghostty "Issue Triage" discussion

Post to: https://github.com/ghostty-org/ghostty/discussions/new?category=issue-triage
Category: **Issue Triage** (per CONTRIBUTING.md, bug reports are discussions, not issues)

**Before posting, read the notes at the bottom of this file.**

---

**Title:** libghostty-vt: snapshot PAGE header capacity is not bounded, decoding a corrupted snapshot writes out of bounds

## Issue Description

`terminal.snapshot.page.Header.pageCapacity` copies the PAGE header's capacity
hints into a `terminal.page.Capacity` and validates only that columns and rows
are nonzero. Nothing checks that the resulting page layout is representable.

`PageList.createPageExt` then treats the limit as an assertion:

```zig
assert(layout.total_size <= size.max_page_size);
```

Page members are addressed with `size.OffsetInt`, a `u32`, so this is the
layout's precondition rather than a diagnostic. `ReleaseFast` drops it, which
lets the surrounding 64-bit layout arithmetic narrow to 32 bits. The page is
then allocated for the truncated size while its `string_alloc` keeps the
advertised capacity, so the first hyperlink string allocation returns a pointer
past the end of the mapping and `readSliceAll` writes through it.

Observed with `capacity.string_bytes = 4294903808` (`0xffff0800`):

| field | value |
| :--- | ---: |
| `page.memory.len` | 16793600 |
| `string_alloc.bitmap_count` | 2097121 |
| `string_alloc.chunks` offset | 16855200 |

The chunks region begins 61,600 bytes past the end of the page memory.

Every capacity field can exceed the limit on its own: `grapheme_capacity_bytes`
and `string_capacity_bytes` are `u32`, and maximum `u16` columns and rows alone
need roughly 34 GB of cells.

This is reachable from any untrusted snapshot, which I think is the intended
threat model for the decoder given the CRC and framing validation elsewhere.

## Expected Behavior

`Decoder.init` rejects the record with a decode error before allocating
anything, the same way it rejects zero dimensions with `error.InvalidDimensions`.

## Actual Behavior

`ReleaseFast`: `SIGSEGV` inside `_platform_memmove`, called from
`terminal.snapshot.hyperlink.decodePageString`.

`Debug`: the `assert` in `createPageExt` aborts.

## Reproduction Steps

1. Add this test to `src/terminal/snapshot/page.zig`:

   ```zig
   test "PAGE header capacity is bounded" {
       const header: Header = .{
           .columns = 82,
           .rows = 83,
           .style_count = 0,
           .hyperlink_count = 1,
           .style_capacity = 128,
           .hyperlink_capacity_bytes = 192,
           .grapheme_capacity_bytes = 8192,
           .string_capacity_bytes = 0xffff0800,
       };

       var encoded: [Header.len]u8 = undefined;
       var writer: std.Io.Writer = .fixed(&encoded);
       try header.encode(&writer);

       var reader: std.Io.Reader = .fixed(writer.buffered());
       // Expected: a decode error. Actual: the capacity is accepted.
       _ = decodePayload(&reader, std.testing.allocator) catch return;
       return error.TestExpectedError;
   }
   ```

2. `zig build test-lib-vt -Dtest-filter="PAGE header capacity is bounded"`

   The assertion in `createPageExt` aborts before the test can report.

3. For the out-of-bounds write rather than the assertion, build
   `-Doptimize=ReleaseFast` and decode a snapshot whose PAGE header carries the
   same `string_capacity_bytes` through
   `ghostty_snapshot_decoder_new_buf` / `ghostty_snapshot_decoder_ready`.

I found this by mutating single bytes of valid snapshots. The two snapshots that
produced this were 5008 bytes and differed only in the PAGE header's
`string_capacity_bytes`; everything else, including the record CRC, was
recomputed to stay valid.

## Ghostty Logs

```text
_platform_memmove
terminal.snapshot.hyperlink.decodePageString + 216
terminal.snapshot.page.decodePayloadBody + 1348
terminal.snapshot.page.Decoder.decode + 460
terminal.snapshot.snapshot.Decoder.ready + 17980
terminal.c.snapshot.decoderReadyTerminal + 96
ghostty_snapshot_decoder_ready + 100

EXC_BAD_ACCESS (SIGSEGV), KERN_INVALID_ADDRESS
```

## Ghostty Version

Not the app — this is `libghostty-vt` built from source:

```text
commit 82232ecde55405559dec29c5466cb9e39938cb41
zig build -Demit-lib-vt -Demit-xcframework=false -Doptimize=ReleaseFast -Dcpu=baseline
Zig 0.16.0
```

`src/terminal/snapshot/page.zig` and `src/terminal/page.zig` are unchanged on
`main` as of this writing, so I believe `main` is affected as well.

## OS Version Information

macOS 26.6, arm64 (Apple silicon)

## Minimal Ghostty Configuration

```ini
# Not applicable: this is libghostty-vt used as a library, no Ghostty config.
```

## Additional Relevant Configuration

None.

---

## Notes before posting (delete this section)

- **You must be able to explain this yourself.** CONTRIBUTING.md's "Critical
  Rule" and AI_POLICY.md both require the human to fully understand the content.
  Read the two functions and the layout math before you post.
- **AI disclosure is mandatory.** Add a line stating the tool and the extent of
  assistance, e.g. "Investigated and drafted with Claude Code; I verified the
  reproduction, the debugger values, and the upstream code myself." Then edit
  this text into your own voice — the policy explicitly asks humans to trim AI
  verbosity.
- **Tick the four acknowledgement checkboxes** only after you have actually
  searched. I searched issues and discussions for `snapshot decoder`,
  `libghostty-vt`, `page capacity`, `stream_continuation` and found no match;
  the closest are #11998 (feature) and #14148 (formatter replay), neither of
  which covers this.
- **Do not open a PR yet.** First-time contributors need a vouch (a "Vouch
  Request" discussion) before PRs are accepted, and Ghostty wants an accepted
  issue before a PR. Mention in the discussion that you have a patch and tests
  if they want them.
- We carry a local fix; it adds `CapacityError.CapacityTooLarge` and measures
  `Page.layout(capacity).total_size` against `size.max_page_size` inside
  `pageCapacity`. Offer it, don't paste it unless asked.
