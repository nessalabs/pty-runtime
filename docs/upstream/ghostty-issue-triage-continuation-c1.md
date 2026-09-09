# DRAFT — Ghostty "Issue Triage" discussion

Post to: https://github.com/ghostty-org/ghostty/discussions/new?category=issue-triage
Category: **Issue Triage** (per CONTRIBUTING.md, bug reports are discussions, not issues)

**Before posting, read the notes at the bottom of this file.**

---

**Title:** libghostty-vt: snapshot encode fails with ReplayWouldCommit when a C1 introducer abandons an unfinished APC string

## Issue Description

`Stream.trackContinuation` picks the retained replay start with
`findVTReplayStart`, which returns the index of the last ESC. That is only the
start of the still-unfinished sequence if the parser stayed outside ground from
that ESC onward. A C1 introducer also abandons the current sequence and begins a
new one, so the retained bytes can still open with a sequence that has since
been completed.

`sos_pm_apc_string` is the case that matters: it emits `apc_end` on exit and,
unlike `dcs_passthrough`, `dcs_ignore`, and `osc_string`, it does not override
the "anywhere" C1 transitions in `parse_table.zig`, so `0x90`, `0x9b`, and
`0x9d` still introduce from inside it.

For `"\x1b_apc\x90qdata"` the retained bytes are the whole string. `0x90` ends
the APC and opens the DCS, so replaying those bytes would run the APC command a
second time. `Tracker.write` cannot omit that byte, because omission is only
safe for bytes that change no parser state tag, and
`stream_continuation.validate` then rejects the exported suffix with
`error.ReplayWouldCommit`. `snapshot.encode` preflights the continuation, so the
whole snapshot encode fails.

No suffix of the original bytes is a valid continuation here. Replay starts from
ground, and from ground a raw `0x90` is ill-formed UTF-8 that decodes to U+FFFD
rather than introducing a DCS, so trimming to the `0x90` loses the state too.

## Expected Behavior

`writeContinuation` produces a suffix that reconstructs the unfinished DCS and
replays nothing already committed, and `ghostty_snapshot_encode` succeeds.

## Actual Behavior

`writeContinuation` returns the untrimmed bytes; `validate` rejects them with
`error.ReplayWouldCommit`, and `ghostty_snapshot_encode` returns
`invalid_value`. The terminal cannot be snapshotted again.

## Reproduction Steps

1. Add this test to `src/terminal/stream.zig`:

   ```zig
   test "continuation after a C1 introducer is exportable" {
       const S = Stream(ContinuationTestHandler);
       var stream = S.init(.{
           .allocator = testing.allocator,
           .handler = .{},
           .continuation_max_bytes = 1024,
       });
       defer stream.deinit();

       // Unfinished APC string, then 0x90 ends it and opens a DCS.
       stream.nextSlice("\x1b_apc\x90qdata");

       var buf: [1024]u8 = undefined;
       var writer: std.Io.Writer = .fixed(&buf);
       try stream.writeContinuation(&writer);

       // Fails: the exported suffix replays the completed APC string.
       try continuationpkg.validate(writer.buffered());
   }
   ```

2. `zig build test-lib-vt -Dtest-filter="continuation after a C1 introducer"`

   ```text
   error: 'terminal.stream.test.continuation after a C1 introducer is exportable' failed:
   error.ReplayWouldCommit
   ```

   `writer.buffered()` is `"\x1b_apc\x90qdata"`; I expected `"\x1bPqdata"`.

`0x9b` and `0x9d` behave the same in place of `0x90`. `0x98`, `0x9e`, and `0x9f`
do not reproduce it from an APC string, because they re-enter the same state and
so emit no exit action.

I found this by feeding random bytes to a `Stream` and snapshotting; the
original case was a 87-byte feed whose parser ended in `dcs_passthrough`, and
exactly one retained byte classified as `.committed`.

## Ghostty Logs

```text
error.ReplayWouldCommit
  terminal.stream_continuation.validate
  terminal.snapshot.continuation.encode
  terminal.snapshot.snapshot.encode
  terminal.c.snapshot.encode  -> Result.invalid_value
```

## Ghostty Version

Not the app — this is `libghostty-vt` built from source:

```text
commit 82232ecde55405559dec29c5466cb9e39938cb41
zig build -Demit-lib-vt -Demit-xcframework=false -Doptimize=ReleaseFast -Dcpu=baseline
Zig 0.16.0
```

`Tracker.write` and `findVTReplayStart` are unchanged on `main` as of this
writing, so I believe `main` is affected as well.

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

- **You must be able to explain this yourself.** Read `Tracker.write`,
  `findVTReplayStart`, `BoundaryScanner.next`, and the `sos_pm_apc_string` block
  of `parse_table.zig` before posting.
- **AI disclosure is mandatory** under AI_POLICY.md. State the tool and the
  extent of assistance, then rewrite this in your own voice and trim it.
- **Acknowledgement checkboxes:** I searched issues and discussions for
  `continuation`, `stream_continuation`, `ReplayWouldCommit`, `snapshot`, and
  `libghostty-vt`; nothing matches. Nearest neighbours are #11998 and #14148.
- **Do not open a PR yet** — vouch first, and Ghostty wants an accepted issue
  before a PR.
- Our local fix trims the retained bytes at export to the sequence still being
  built and spells a C1 introducer as `ESC` + (byte − 0x40), so `0x90` becomes
  `ESC P`. It only trims when the untrimmed bytes would replay committed work,
  so inert abandoned prefixes still export byte-identically and the existing
  `"text\x1b[12\x9d2;title"` expectation is unchanged. Offer it if they want it.
- One design question worth asking them rather than asserting: whether they
  prefer the synthesized seven-bit introducer, or would rather widen the
  continuation contract some other way. It changes what a restored stream
  re-exports.
