# Terminal client protocol, version 1

A demonstration protocol carrying a live terminal between the runtime and a
browser. It is deliberately small and readable: JSON text frames over one
WebSocket, one session per socket.

Every message carries `v`. A client that receives a `v` it does not know must
close the socket rather than guess, and the server does the same. Version 1 is
frozen; a change that is not backward compatible takes `v: 2`.

The server renders. The browser receives grid cells that are already laid out,
so it needs no terminal emulator, no escape-sequence parser, and no scrollback
buffer of its own. That is the point of the design: the runtime's Ghostty
engine is the single source of truth for what the screen looks like.

## Transport

`GET /ws` upgrades to a WebSocket. All frames are UTF-8 JSON text.

`GET /` serves the demo page and `GET /terminal.js` the component.

## Client to server

### hello

Must be the first message. The server replies with `ready`, then `frame`.

```json
{"v": 1, "type": "hello", "cols": 80, "rows": 24}
```

`cols` and `rows` are 1..=1000 and their product must not exceed 1048576, which
is the runtime's own grid limit.

### input

Bytes for the child process. `data` is base64 because terminal input is not
always valid UTF-8: a paste can carry arbitrary bytes, and key encodings such
as `ESC [ A` are byte sequences rather than text.

```json
{"v": 1, "type": "input", "data": "bHMK"}
```

### resize

```json
{"v": 1, "type": "resize", "cols": 120, "rows": 40}
```

### history

Ask for retained rows. `start` addresses the engine's current window, where
index zero is the oldest row still held.

```json
{"v": 1, "type": "history", "start": 120, "count": 40}
```

The server clamps `start` into the window and `count` to a bounded maximum, so
a request that has aged out returns what survives rather than failing.

## Server to client

### ready

```json
{"v": 1, "type": "ready", "cols": 80, "rows": 24, "palette": {...}}
```

`palette` carries the effective default foreground, background and cursor
colors, plus the 256 indexed colors, each as `[r, g, b]` or `null` when the
terminal defers to the client. It is sent once; an OSC change re-sends it.

### frame

A frame is a set of changed rows plus cursor state. The server diffs against
what it last sent on this socket, so an idle terminal produces no frames and a
single changed line produces one row.

```json
{
  "v": 1,
  "type": "frame",
  "seq": 12,
  "cursor": {"col": 3, "row": 7, "visible": true},
  "rows": [{"y": 7, "cells": [[" ", 1, 0], ["h", 1, 5]]}]
}
```

`seq` increases by one per frame and lets a client detect a gap.

A cell is `[text, width, style]`:

- `text` is the grapheme, which may be several code points (`e` plus a
  combining accent) or empty for an unoccupied cell.
- `width` is 0, 1 or 2. A 0 marks the spacer cell that follows a wide glyph;
  the client must not draw it.
- `style` is an index into the frame's `styles` table when present, or 0 for
  the default style.

### styles

Sent immediately before the first `frame` that references a new style, so a
client can keep one table for the life of the socket.

```json
{"v": 1, "type": "styles", "styles": [{"id": 5, "fg": [255,0,0], "bold": true}]}
```

Absent fields take their default: `fg` and `bg` default to the palette
default, and every boolean defaults to false. `underline` is one of `none`,
`single`, `double`, `curly`, `dotted`, `dashed`.

### modes

Input-affecting state the client needs to encode keys correctly.

```json
{"v": 1, "type": "modes", "application_cursor": true, "bracketed_paste": true,
 "alternate_screen": false, "mouse": "button_motion", "mouse_encoding": "sgr"}
```

`application_cursor` changes arrow keys from `ESC [ A` to `ESC O A`, and
`bracketed_paste` wraps a paste in `ESC [ 200 ~` and `ESC [ 201 ~`. A client
that ignores these still works; it just sends the wrong bytes in those modes.

`mouse` and `mouse_encoding` are separate questions, and a client that infers
one from the other will send a program bytes it cannot parse.

`mouse` is which events the program asked for:

| Value | Mode | Reports |
| :--- | ---: | :--- |
| `none` | | nothing |
| `press` | 9 | presses only |
| `press_release` | 1000 | presses and releases |
| `button_motion` | 1002 | presses, releases, motion while held |
| `any_motion` | 1003 | presses, releases, all motion |

`mouse_encoding` is how they are written: `sgr` (mode 1006) is
`ESC [ < button ; col ; row M` for a press and `m` for a release with 1-based
coordinates; `legacy` is `ESC [ M` and three bytes biased by 32, which cannot
express a coordinate past 223.

A client must send only what the tracking mode asked for. Sending motion to a
program that requested `press_release` is not merely wasteful: it is input the
program never agreed to parse.

`alternate_screen` also matters for input, not just display: when a program is
on the alternate screen and has *not* asked for mouse tracking, the reference
client turns wheel movement into cursor keys. That is what makes the wheel
scroll in vim and less, and it is what terminal emulators do for the same
reason. Off the alternate screen a wheel does nothing, because there is no
scrollback in this version to move through.

### history

The reply to a `history` request. Rows use the same encoding as a `frame`, so a
client parses one shape for both, and `y` is the absolute row index rather than
a screen position.

```json
{"v": 1, "type": "history", "start": 120, "rows": [{"y": 120, "cells": []}],
 "total": 812, "scrollback": 788}
```

`start` is where the read actually began after clamping. `total` and
`scrollback` describe the current window: indices are **window-relative and
shift as old rows are discarded**, so a client holding a scroll position
re-anchors from these rather than assuming its indices remain valid. ADR 0006
records why the engine cannot offer a stable numbering.

Reading history moves no viewport and has no effect a later read or checkpoint
can observe, so several clients may read different ranges at once.

### exit

```json
{"v": 1, "type": "exit", "status": 0}
```

`status` is the child's exit code, or null when it was signalled.

### error

```json
{"v": 1, "type": "error", "message": "session write failed"}
```

Terminal to the socket: the server closes after sending it.

## What version 1 does not do

Selection and reconnect are not in this version. Reconnect is the interesting
one, because the runtime already has it: the event stream carries replay
cursors that survive a dropped connection, so a version 2 could resume from a
cursor instead of starting a new session.

Scrollback is carried, but only as whole-window reads. A client that scrolls
re-requests the rows it needs; there is no incremental update of a scrolled
view, so a program writing while a viewer is scrolled back does not refresh
what that viewer sees until the next request.

Mouse input is carried, but as ordinary `input` bytes encoded by the client
rather than as a message of its own. A version 2 that reported the individual
tracking modes could move that encoding to the server, where the mode is
known.
