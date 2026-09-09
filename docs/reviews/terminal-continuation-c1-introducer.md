# Continuation export must begin at the sequence still unfinished

Status: author targeted tests pass, seed 509 passes, and the seeds 1-1000
corpus completes for the first time, all recorded under
`docs/verification/continuation-c1/`. Root owns independent DDD, design, and
correctness review, and the Linux gate. No release acceptance is claimed.

Both native defects corrected today are reported upstream as Ghostty
discussions 14185 and 14186.

## Failure

Corpus seed 509 aborts with `EngineFailure` from
`ITerminal::checkpoint`. Unlike the page-capacity defect this is not a crash:
`ghostty_snapshot_encode` returns `error.ReplayWouldCommit`, which
`mapEncodeError` maps to `invalid_value`, `rt_checkpoint` reports `-1`, and
`state.rs` converts it to `TerminalError::EngineFailure` while setting
`failed = true`, so the terminal can never be checkpointed again.

The failure is independent of the page-capacity correction. Rebuilding the
library without that correction reproduces it identically
(`seed509-pre-existing.log`).

## Cause

Four feeds reproduce it. The state after each, from a throwaway `Stream` test
retained as `native-tests/diagnosis-repro.zig.txt`:

| feed | parser state after | retained |
| ---: | :--- | ---: |
| `1b 5b 48 1b 5b 32 4a` | ground | 0 |
| `e2 82` | ground, UTF-8 pending | 2 |
| `1b 5b 33 38 3b 32 3b 31 30 3b` | csi_param | 10 |
| 87 arbitrary bytes | dcs_passthrough | 71 |

The last feed ends inside a DCS, so `Stream.trackContinuation` takes the `.vt`
path and `findVTReplayStart` returns the index of the last ESC. That ESC opened
an SOS/PM/APC string which did not stay open: the raw C1 byte `0x90` later ended
it and opened the DCS. Scanning the retained 71 bytes classifies exactly one
byte as `.committed`, `0x90` at index 11, moving `sos_pm_apc_string` to
`dcs_entry` and emitting `apc_end`.

The retained suffix therefore replays an APC command the terminal already
executed. `Tracker.write` cannot omit it, because omission is only safe for a
byte that changes no parser state tag, and `validate` correctly rejects the
result.

No suffix of the original bytes is a valid continuation here. Replay begins at
ground, and from ground a raw `0x90` is ill-formed UTF-8 that decodes to U+FFFD
rather than introducing a DCS, so starting at index 11 loses the state and
starting at index 12 loses it as well.

`sos_pm_apc_string` is the only parser state that both emits a committed exit
action and leaves the "anywhere" C1 transitions in place; `dcs_passthrough`,
`dcs_ignore`, and `osc_string` all keep high bytes as payload. So the reachable
shape is an unfinished APC string followed by `0x90`, `0x9b`, or `0x9d`.

## Bounded correction

`Tracker.write` now trims the retained bytes to the sequence the parser is
still building, and spells a C1 introducer as its seven-bit equivalent, `ESC`
followed by the byte minus `0x40`. Here `0x90` becomes `ESC P`, so replay from
ground reaches `dcs_passthrough` with the same payload and commits nothing.

`replayStart` finds that sequence with the existing `BoundaryScanner`, so
export, validation, and the feed path share one model. A byte introduces when
it is ESC, or when it is a C1 introducer whose processing actually moved the
parser into that introducer's entry state; the string states that keep high
bytes as payload therefore never match. Trimming applies only when the retained
bytes would otherwise replay committed work, so an abandoned but inert prefix
still exports byte-identically, and the existing golden expectation for
`"text\x1b[12\x9d2;title"` is unchanged.

The feed path is untouched: `findVTReplayStart` and its SIMD scan still record
the retained start, and the new scan runs only at export.

Export never exceeds the retained length. The retained bytes always begin at an
ESC or a UTF-8 lead byte, so a C1 introducer is never at index zero, and
replacing one byte at index one or later with two bytes cannot grow the result.

Patch SHA-256: `745b5e98703259704c7ad9a5c2e1357817efda85cb6f60cd3db730d6d42f4947`.
Corrected `stream_continuation.zig` SHA-256:
`8d36a4991ce7a9432857e2d12052d5f212c0faac619ff3e38901b11ada726734`.
Corrected `stream.zig` SHA-256:
`7e2f63d504bdf558de243b837017a834a951faeecbe7691218a217b11d75f6bc`.
The fixed patch now changes six files. Note that the file is `stream.zig`, not
`Stream.zig`: a case-insensitive macOS filesystem hid the real archive path
during development, and the recorded target path is the archive's.

## Verification

macOS arm64, pinned Zig 0.16.0, baseline CPU configuration.

- Native red/green: dropping only the C1 introducer result from `replayStart`
  fails the new test (`mutation-red.log`, 33 pass / 1 fail). Restored, the
  unfiltered `zig build test-lib-vt` exits 0 (`full-green.log`), and the
  `continuation` filter passes 177/177 over 46/46 build steps.
- The new table test covers `0x90`, `0x9b`, and `0x9d` after an unfinished APC
  string and the ESC-introduced case, asserts each exported suffix validates,
  and asserts a restored stream re-exports it byte-identically. A second test
  pins the inert-prefix behavior that must not change.
- `zig fmt --check` on both corrected sources: exit 0.
- The patch applies cleanly to pristine archive copies and reproduces all six
  corrected files exactly.
- Seed 509 passes with 102 malformed mutations rejected
  (`seed509-after.log`); seed 201 still passes.
- Corpus seeds 1-1000 passes in 2210 s with an unchanged source inventory
  (`corpus-1-1000/`): 32069 feeds, 15925 resizes, 17006 restores, and 102036
  malformed mutations of which 101771 were rejected and 265 decoded into
  bounded usable state. This is the first complete run of the range; earlier
  attempts stopped at seed 201 and then seed 509.
- `python3 scripts/gate.py` results are recorded in `macos-gate/`.

This is scoped evidence for one continuation-export defect. It is not a
complete native corpus beyond the recorded range, and no release acceptance
follows.
