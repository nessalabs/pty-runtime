# Projected modes must preserve distinctions a consumer acts on

Status: implemented and gated on macOS; recorded here because it changes a
public domain type. Root owns independent review and the Linux gate.

## Why this is a contract question, not a convenience

ADR 0005 requires that the adapter "converts cursor, screen, style, mode, and
reply information into our terminal models", and G2-06 requires that views
"expose dimensions/cursor visibility/active screen/modes/text/styles".

`TerminalModes` reported `mouse_reporting: bool`, taken from
`GHOSTTY_TERMINAL_DATA_MOUSE_TRACKING`, which answers only whether *some*
tracking mode is active. That is a lossy conversion of information a consumer
must act on, and the loss is not recoverable downstream:

- A program selects **which events** it wants. X10 (mode 9) asks for presses
  only; 1000 adds releases; 1002 adds motion while a button is held; 1003 adds
  all motion. Sending a program events it did not request is not merely
  wasteful, it is input it never agreed to parse.
- A program separately selects **how those events are encoded**. Mode 1006 is
  SGR; without it the original encoding applies, which cannot express a
  coordinate past 223.

Neither can be inferred from the other, and neither can be inferred from a
boolean. A consumer given only that boolean has to guess, and a wrong guess
sends a running program bytes it will misparse. The runtime was therefore not
meeting the conversion the ADR describes; it was summarizing it.

This was found by building a terminal client against the runtime, which is the
first consumer to need mouse input. The gap was invisible while nothing
consumed it.

## Change

`TerminalModes` now carries the two questions separately:

```rust
pub enum MouseTracking { None, Press, PressRelease, ButtonMotion, AnyMotion }
pub enum MouseEncoding { Legacy, Sgr }
```

`mouse_reporting` remains available as a **method** rather than a field, derived
from the tracking mode. Keeping it as stored state alongside the enum would
allow the two to disagree; deriving it means the summary cannot contradict what
it summarizes.

The adapter queries the individual DEC modes through
`GHOSTTY_TERMINAL_DATA_MODE`, reusing the `mode()` helper already present in the
C view shim for bracketed paste and cursor keys. No new native mechanism was
added. `RuntimeInfo` carries five flags in place of the previous aggregate.

Later tracking modes win when several are set, because a program that enables
any-event tracking over button tracking wants the wider set.

## Compatibility

This is a breaking change to a public domain type. `TerminalModes` gains two
fields and loses one, and `modes.mouse_reporting` becomes
`modes.mouse_reporting()`. Three call sites in the workspace were affected. The
crate is unreleased and release acceptance is incomplete, so the break is taken
now rather than carrying a redundant field into a release.

It is not a checkpoint-format change: modes are projected state, not encoded
snapshot state, so the compatibility marker is unaffected.

## Verification

- A new contract test proves the distinction is real rather than merely
  represented: 1002 with 1006 reports `ButtonMotion` and `Sgr`, 1003 supersedes
  the tracking mode while leaving the encoding alone, and disabling tracking
  reports `None` with `mouse_reporting()` false. The existing test that feeds
  mode 1000 now asserts `PressRelease` and `Legacy` rather than a boolean.
- The C boundary contract pins which native calls `rt_info` makes. It was
  updated deliberately, not incidentally: the aggregate query is gone and five
  mode queries replace it, and a failed query must report every mouse mode as
  unset rather than defaulting to enabled.
- `python3 scripts/gate.py` passes with an unchanged source inventory, recorded
  in `docs/verification/mouse-modes/macos-gate/`.

## What this does not settle

The reference client in `client/` now encodes SGR or legacy according to the
reported encoding, and withholds events the tracking mode did not request. That
is a consumer decision and is not part of the runtime contract; a different
consumer may choose differently with the same information.

Modes 1005, 1015 and 1016 are not reported. They are alternative encodings that
this runtime's consumers have not needed, and adding them is additive rather
than another breaking change.
