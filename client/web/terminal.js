// A React terminal backed by the PTY runtime.
//
// The server renders, so this component never parses an escape sequence. It
// receives grid rows that are already laid out, paints them, and sends key
// bytes back. That is why it is this small: everything hard about terminal
// emulation happens in the runtime, not here.
//
// No build step and no dependency beyond React itself, so it can be dropped
// into any app:
//
//   import { Terminal } from "./terminal.js";
//   <Terminal url="ws://localhost:7749/ws" />
//
// Protocol: ../PROTOCOL.md

import React from "https://esm.sh/react@18.3.1";

const { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } = React;
const h = React.createElement;

const PROTOCOL_VERSION = 1;

/** Encode bytes as base64 without assuming the input is text. */
function toBase64(bytes) {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

const encoder = new TextEncoder();

// Keys that end in `~` carry a number; the CSI parameter is that number.
const TILDE_KEYS = {
  Insert: 2,
  Delete: 3,
  PageUp: 5,
  PageDown: 6,
  F5: 15,
  F6: 17,
  F7: 18,
  F8: 19,
  F9: 20,
  F10: 21,
  F11: 23,
  F12: 24,
};

// Keys that end in a letter. Unmodified they may use SS3 (`ESC O A`);
// modified they always use CSI with a parameter.
const LETTER_KEYS = {
  ArrowUp: "A",
  ArrowDown: "B",
  ArrowRight: "C",
  ArrowLeft: "D",
  End: "F",
  Home: "H",
  F1: "P",
  F2: "Q",
  F3: "R",
  F4: "S",
};

/**
 * Whether this is an Apple keyboard layout, which changes two things only.
 *
 * Option composes characters here and nowhere else, and Command is a terminal
 * modifier here while Super elsewhere belongs to the window manager. Nothing
 * else in this file is platform dependent: the escape sequences a terminal
 * expects are the same on Linux, macOS and Windows, because they come from
 * the VT and xterm lineage rather than from the operating system.
 */
const IS_APPLE = /Mac|iPhone|iPad/.test(
  globalThis.navigator?.userAgentData?.platform ??
    globalThis.navigator?.platform ??
    "",
);

/**
 * Clipboard chords belong to the browser, not the PTY.
 *
 * On Apple, Command-C/V/X/A. Elsewhere, Ctrl+Shift+C/V (and Ctrl+C only when
 * there is a selection so bare Ctrl+C remains interrupt).
 */
export function isClipboardShortcut(event) {
  const key = event.key.length === 1 ? event.key.toLowerCase() : "";
  if (!key) return false;
  if (IS_APPLE) {
    if (!(event.metaKey && !event.ctrlKey && !event.altKey)) return false;
    return key === "c" || key === "v" || key === "x" || key === "a";
  }
  if (!(event.ctrlKey && !event.metaKey && !event.altKey)) return false;
  if (key === "v") return true;
  if (key === "c") {
    if (event.shiftKey) return true;
    const selection = globalThis.getSelection?.();
    return Boolean(selection && !selection.isCollapsed && selection.toString());
  }
  return false;
}

/**
 * xterm's modifier parameter: 1 plus a bitmask, so unmodified is 1.
 *
 * Applications read this to tell Shift+Left from Left, which is how editors
 * implement selection with the keyboard.
 */
function modifierParameter(event) {
  return (
    1 +
    (event.shiftKey ? 1 : 0) +
    (event.altKey ? 2 : 0) +
    (event.ctrlKey ? 4 : 0) +
    (event.metaKey ? 8 : 0)
  );
}

/**
 * Translate a keydown into the bytes a terminal expects.
 *
 * `applicationCursor` matters: in that mode arrow keys are `ESC O A` rather
 * than `ESC [ A`, and full-screen programs read the difference.
 */
export function keyBytes(event, applicationCursor) {
  // Leave copy/paste/select-all for the browser; otherwise Command-V is sent
  // as a literal "v" and preventDefault kills the paste event.
  if (isClipboardShortcut(event)) return null;

  const { key, ctrlKey, altKey } = event;
  const modifier = modifierParameter(event);

  if (key === "Tab" && event.shiftKey) return encoder.encode("\x1b[Z");

  if (TILDE_KEYS[key] !== undefined) {
    const number = TILDE_KEYS[key];
    return encoder.encode(
      modifier === 1 ? `\x1b[${number}~` : `\x1b[${number};${modifier}~`,
    );
  }

  if (LETTER_KEYS[key] !== undefined) {
    const letter = LETTER_KEYS[key];
    if (modifier !== 1) return encoder.encode(`\x1b[1;${modifier}${letter}`);
    // F1-F4 are always SS3; arrows and Home/End follow the cursor-key mode.
    const ss3 = "PQRS".includes(letter) || applicationCursor;
    return encoder.encode(ss3 ? `\x1bO${letter}` : `\x1b[${letter}`);
  }

  // Backspace carries three separate meanings on a Mac keyboard, and only the
  // plain one is a terminal convention. Option-Backspace is ESC DEL, which
  // readline and zsh read as backward-kill-word. Command-Backspace has no
  // encoding at all, so terminals map it to the kill-line control the shell
  // already understands, and this does the same.
  if (key === "Backspace") {
    // Command-Backspace is an Apple convention with no encoding of its own, so
    // it becomes the kill-line control the shell already understands. Super on
    // other platforms is the window manager's and is left alone.
    if (event.metaKey) return IS_APPLE ? new Uint8Array([0x15]) : null;
    // Alt-Backspace is backward-kill-word to readline and zsh on every
    // platform. Ctrl-Backspace is what xterm sends for the same intent.
    if (altKey) return new Uint8Array([0x1b, 0x7f]);
    if (ctrlKey) return new Uint8Array([0x08]);
    return new Uint8Array([0x7f]);
  }

  const named = {
    Enter: "\r",
    Tab: "\t",
    Escape: "\x1b",
  };
  if (named[key] !== undefined) return encoder.encode(named[key]);

  // With Option held, Apple keyboards report the composed character rather
  // than the key: Option-B arrives as the integral sign. The base letter then
  // survives only in the physical code.
  //
  // `code` is a physical position, so it is wrong on Dvorak, AZERTY and every
  // other non-QWERTY layout. It is therefore a fallback, not the primary: when
  // `key` is already a plain ASCII character it is what the user's layout
  // actually produced and is preferred.
  const ascii = key.length === 1 && key.charCodeAt(0) < 128 ? key : null;
  const physical = /^(?:Key([A-Z])|Digit([0-9]))$/.exec(event.code ?? "");
  const base =
    ascii ?? (physical ? (physical[1] ?? physical[2]).toLowerCase() : null);

  if (ctrlKey && base) {
    const letter = base.toUpperCase();
    const code = letter.charCodeAt(0);
    // Ctrl-A through Ctrl-Z, plus the punctuation controls above them.
    if (code >= 64 && code < 96) return new Uint8Array([code - 64]);
    if (key === " ") return new Uint8Array([0]);
  }
  if (altKey && base) {
    // Meta is ESC then the unmodified key, which is how shells see Alt.
    return new Uint8Array([0x1b, ...encoder.encode(base)]);
  }
  if (key.length === 1) {
    const bytes = encoder.encode(key);
    return altKey ? new Uint8Array([0x1b, ...bytes]) : bytes;
  }
  return null;
}

/**
 * Encode a mouse event in the encoding the program actually asked for.
 *
 * SGR is `ESC [ < button ; col ; row M` for a press and `m` for a release.
 * The legacy encoding is `ESC [ M` followed by three bytes biased by 32,
 * which is why it cannot express a coordinate past 223; a program that never
 * enabled SGR gets a clamped coordinate rather than a corrupt sequence.
 */
export function mouseBytes({
  type,
  button,
  col,
  row,
  shift,
  alt,
  ctrl,
  encoding = "sgr",
}) {
  let code;
  if (type === "wheel") {
    code = button === "up" ? 64 : 65;
  } else if (type === "up" && encoding !== "sgr") {
    // The legacy encoding has no release button; 3 means "some button up".
    code = 3;
  } else {
    code = { left: 0, middle: 1, right: 2 }[button] ?? 0;
    if (type === "move") code += 32;
  }
  if (shift) code += 4;
  if (alt) code += 8;
  if (ctrl) code += 16;

  if (encoding === "sgr") {
    const final = type === "up" ? "m" : "M";
    return encoder.encode(`\x1b[<${code};${col + 1};${row + 1}${final}`);
  }
  const clamp = (value) => Math.min(value + 33, 255);
  return new Uint8Array([0x1b, 0x5b, 0x4d, code + 32, clamp(col), clamp(row)]);
}

function cssColor(rgb, fallback) {
  if (!rgb) return fallback;
  return `rgb(${rgb[0]},${rgb[1]},${rgb[2]})`;
}

function samePalette(a, b) {
  if (a === b) return true;
  if (!a || !b) return false;
  if (a.foreground !== b.foreground && JSON.stringify(a.foreground) !== JSON.stringify(b.foreground)) {
    return false;
  }
  if (a.background !== b.background && JSON.stringify(a.background) !== JSON.stringify(b.background)) {
    return false;
  }
  if (a.cursor !== b.cursor && JSON.stringify(a.cursor) !== JSON.stringify(b.cursor)) {
    return false;
  }
  const left = a.indexed || [];
  const right = b.indexed || [];
  if (left.length !== right.length) return false;
  for (let i = 0; i < left.length; i++) {
    const x = left[i];
    const y = right[i];
    if (x === y) continue;
    if (!x || !y || x[0] !== y[0] || x[1] !== y[1] || x[2] !== y[2]) return false;
  }
  return true;
}

function pasteTextPayload(text, bracketed) {
  const wrapped = bracketed ? `\x1b[200~${text}\x1b[201~` : text;
  return toBase64(encoder.encode(wrapped));
}

/** Quote a filesystem path for the shell when it needs it. */
function shellPath(path) {
  if (/^[A-Za-z0-9_./:@%+=,-]+$/.test(path)) return path;
  return `'${path.replace(/'/g, `'\"'\"'`)}'`;
}

/** Derive the paste upload URL from the WebSocket endpoint. */
function pasteEndpoint(wsUrl) {
  const url = new URL(wsUrl, globalThis.location?.href ?? "http://127.0.0.1/");
  url.protocol = url.protocol === "wss:" ? "https:" : "http:";
  url.pathname = "/paste-file";
  url.search = "";
  url.hash = "";
  return url.toString();
}

/** Keep overlapping rows across a geometry change instead of flashing blank. */
function adaptLines(previous, cols, rows) {
  return Array.from({ length: rows }, (_, y) => {
    const row = previous.lines[y];
    if (!row) return [];
    if (row.length === cols) return row;
    return row.length > cols ? row.slice(0, cols) : row;
  });
}

/** Turn a wire style into inline CSS, resolving inverse against the palette. */
function styleToCss(style, palette) {
  if (!style) return null;
  const defaultFg = cssColor(palette?.foreground, "#e6e6e6");
  const defaultBg = cssColor(palette?.background, "#101014");
  let fg = cssColor(style.fg, defaultFg);
  let bg = cssColor(style.bg, defaultBg);
  if (style.inverse) [fg, bg] = [bg, fg];

  const css = { color: fg };
  // Only emit a background when it differs, so the common case stays cheap.
  if (bg !== defaultBg || style.inverse) css.background = bg;
  if (style.bold) css.fontWeight = "bold";
  if (style.italic) css.fontStyle = "italic";
  if (style.faint) css.opacity = 0.6;
  if (style.invisible) css.visibility = "hidden";

  const lines = [];
  if (style.underline) lines.push("underline");
  if (style.strikethrough) lines.push("line-through");
  if (style.overline) lines.push("overline");
  if (lines.length) {
    css.textDecorationLine = lines.join(" ");
    if (style.underline === "double") css.textDecorationStyle = "double";
    else if (style.underline === "curly") css.textDecorationStyle = "wavy";
    else if (style.underline === "dotted") css.textDecorationStyle = "dotted";
    else if (style.underline === "dashed") css.textDecorationStyle = "dashed";
  }
  return css;
}

/**
 * Collapse a row into runs sharing one style, each occupying exactly the
 * columns it owns.
 *
 * A row is 80 or more cells but usually only a handful of styles, so grouping
 * is the difference between 80 DOM nodes per row and two or three.
 *
 * Each run is then given an explicit width in cells rather than being allowed
 * to flow. Text flow advances by font metrics, and a font's double-width
 * glyphs are not exactly twice its ASCII advance: CJK and emoji drift, and
 * because flow is cumulative the drift grows along the row. Measured before
 * this, a line of ten CJK characters landed three and a half columns left of
 * where the grid put it. Pinning each run's width confines any mismatch to
 * the glyph itself.
 *
 * Wide glyphs are additionally given a run of their own, so a two-column box
 * is never averaged into its neighbours.
 */
function runs(cells, palette, styles, cellWidth) {
  const out = [];
  let current = null;
  const flush = () => {
    if (current) out.push(current);
    current = null;
  };
  for (const [text, width, styleId] of cells) {
    // Width zero is the spacer that follows a wide glyph; the glyph itself
    // already covers both columns, so drawing the spacer would double it.
    if (width === 0) continue;
    const glyph = text === "" ? " " : text;
    if (width === 2) {
      flush();
      out.push({ styleId, text: glyph, cells: 2 });
      continue;
    }
    if (current && current.styleId === styleId) {
      current.text += glyph;
      current.cells += 1;
      continue;
    }
    flush();
    current = { styleId, text: glyph, cells: 1 };
  }
  flush();
  return out.map((run, index) =>
    h(
      "span",
      {
        key: index,
        style: {
          ...styleToCss(styles.get(run.styleId), palette),
          display: "inline-block",
          width: `${run.cells * cellWidth}px`,
          overflow: "hidden",
          whiteSpace: "pre",
          // A terminal has no bidi algorithm: cell zero is drawn leftmost
          // whatever script it holds. Left to itself the browser reorders
          // Arabic and Hebrew, which was measured putting the first character
          // of a line a hundred pixels right of its last. Overriding bidi
          // restores memory order, which is what the grid means.
          direction: "ltr",
          unicodeBidi: "bidi-override",
        },
      },
      run.text,
    ),
  );
}

export function Terminal({
  url = `ws://${location.host}/ws`,
  pasteUrl,
  cols: fixedCols,
  rows: fixedRows,
  fontFamily = "ui-monospace, SFMono-Regular, Menlo, monospace",
  fontSize = 14,
  onExit,
}) {
  const containerRef = useRef(null);
  const socketRef = useRef(null);
  const stylesRef = useRef(new Map());
  const modesRef = useRef({
    application_cursor: false,
    bracketed_paste: false,
    alternate_screen: false,
    mouse: "none",
    mouse_encoding: "sgr",
  });

  const [grid, setGrid] = useState({ cols: 0, rows: 0, lines: [] });
  const [cursor, setCursor] = useState({ col: 0, row: 0, visible: true });
  const [palette, setPalette] = useState(null);
  const [status, setStatus] = useState("connecting");
  // Scrollback. `offset` is how many rows above the live screen the viewer is
  // looking; zero means following live output. The rows themselves are held by
  // absolute index so an eviction that shifts the window is visible rather
  // than silently renumbering what is on screen.
  const [scroll, setScroll] = useState({
    offset: 0,
    rows: null,
    start: 0,
    total: 0,
    scrollback: 0,
    screenRows: 0,
  });


  // Measure one character so the grid can be sized to the container. Terminal
  // layout is entirely determined by the cell box, so this is the only
  // measurement the component needs.
  const [cell, setCell] = useState({ width: 8, height: 17 });
  const lastSize = useRef(null);
  useLayoutEffect(() => {
    const probe = document.createElement("span");
    probe.style.cssText = `position:absolute;visibility:hidden;font-family:${fontFamily};font-size:${fontSize}px;line-height:1.2`;
    probe.textContent = "M".repeat(100);
    document.body.appendChild(probe);
    const box = probe.getBoundingClientRect();
    document.body.removeChild(probe);
    // Quantize so subpixel font metrics cannot chatter across cell edges.
    setCell({
      width: Math.round((box.width / 100) * 1000) / 1000,
      height: Math.round(box.height * 1000) / 1000,
    });
  }, [fontFamily, fontSize]);

  const send = useCallback((message) => {
    const socket = socketRef.current;
    if (socket && socket.readyState === WebSocket.OPEN) {
      socket.send(JSON.stringify({ v: PROTOCOL_VERSION, ...message }));
    }
  }, []);

  // Measured on demand rather than during render: the container ref is still
  // null on the first pass, so a value computed there would announce a
  // fallback size and leave the grid narrower than the window.
  const cellRef = useRef(cell);
  cellRef.current = cell;
  const measure = useCallback(
    (width, height) => {
      if (fixedCols && fixedRows) return { cols: fixedCols, rows: fixedRows };
      const element = containerRef.current;
      const box = cellRef.current;
      const w = width ?? element?.clientWidth;
      const h = height ?? element?.clientHeight;
      if (!w || !h) return { cols: 80, rows: 24 };
      // One-pixel inset avoids floor chatter when the box sits on a cell edge
      // during continuous window drags.
      const cols = Math.max(1, Math.floor(Math.max(0, w - 1) / box.width));
      const rows = Math.max(1, Math.floor(Math.max(0, h - 1) / box.height));
      const last = lastSize.current;
      if (!last) return { cols, rows };
      // Hysteresis: stay on the announced geometry until half a cell of slack
      // clearly crosses the boundary. Stops N↔N±1 flicker mid-drag.
      let nextCols = cols;
      let nextRows = rows;
      if (cols < last.cols && w + box.width * 0.5 >= last.cols * box.width) {
        nextCols = last.cols;
      } else if (cols > last.cols && w < (last.cols + 0.5) * box.width) {
        nextCols = last.cols;
      }
      if (rows < last.rows && h + box.height * 0.5 >= last.rows * box.height) {
        nextRows = last.rows;
      } else if (rows > last.rows && h < (last.rows + 0.5) * box.height) {
        nextRows = last.rows;
      }
      return { cols: nextCols, rows: nextRows };
    },
    [fixedCols, fixedRows],
  );

  useEffect(() => {
    const socket = new WebSocket(url);
    socketRef.current = socket;

    socket.onopen = () => {
      setStatus("open");
      const size = measure();
      socket.send(
        JSON.stringify({ v: PROTOCOL_VERSION, type: "hello", ...size }),
      );
    };
    socket.onclose = () => setStatus("closed");
    socket.onerror = () => setStatus("error");

    socket.onmessage = (event) => {
      const message = JSON.parse(event.data);
      if (message.v !== PROTOCOL_VERSION) {
        setStatus(`unsupported protocol version ${message.v}`);
        socket.close();
        return;
      }
      switch (message.type) {
        case "ready":
          setPalette((previous) => {
            // Size-only ready keeps style ids; a new palette must not.
            if (!samePalette(previous, message.palette)) {
              stylesRef.current = new Map();
            }
            return message.palette;
          });
          setScroll((previous) => ({ ...previous, screenRows: message.rows }));
          setGrid((previous) =>
            previous.cols === message.cols && previous.rows === message.rows
              ? previous
              : {
                  cols: message.cols,
                  rows: message.rows,
                  lines: adaptLines(previous, message.cols, message.rows),
                },
          );
          lastSize.current = { cols: message.cols, rows: message.rows };
          break;
        case "styles":
          for (const style of message.styles) stylesRef.current.set(style.id, style);
          break;
        case "modes":
          modesRef.current = message;
          break;
        case "frame":
          setCursor(message.cursor);
          setScroll((previous) =>
            previous.total === message.total &&
            previous.scrollback === message.scrollback
              ? previous
              : {
                  ...previous,
                  total: message.total,
                  scrollback: message.scrollback,
                },
          );
          if (message.rows.length) {
            setGrid((previous) => {
              const lines = previous.lines.slice();
              for (const row of message.rows) lines[row.y] = row.cells;
              return { ...previous, lines };
            });
          }
          break;
        case "history": {
          const rows = new Map();
          for (const row of message.rows) rows.set(row.y, row.cells);
          setScroll((previous) => ({
            ...previous,
            rows,
            start: message.start,
            total: message.total,
            scrollback: message.scrollback,
          }));
          break;
        }
        case "exit":
          setStatus(`exited (${message.status ?? "signal"})`);
          onExit?.(message.status);
          socket.close();
          break;
        case "error":
          setStatus(`error: ${message.message}`);
          socket.close();
          break;
      }
    };

    return () => socket.close();
    // One socket per url; geometry changes are sent as resize messages.
  }, [url]);

  // Follow the container. The server is authoritative about the grid, so this
  // only asks; the answer arrives as a `ready` message.
  useEffect(() => {
    if (fixedCols && fixedRows) return;
    const element = containerRef.current;
    if (!element || typeof ResizeObserver === "undefined") return;
    let frame = 0;
    const announce = (width, height) => {
      const size = measure(width, height);
      // Announce only real changes; a ResizeObserver fires for any layout
      // pass, and a resize is disruptive to a full-screen program.
      if (size.cols === lastSize.current?.cols && size.rows === lastSize.current?.rows) {
        return;
      }
      lastSize.current = size;
      send({ type: "resize", ...size });
    };
    const observer = new ResizeObserver((entries) => {
      const entry = entries[0];
      let width = element.clientWidth;
      let height = element.clientHeight;
      const box = entry?.contentBoxSize?.[0];
      if (box) {
        width = box.inlineSize;
        height = box.blockSize;
      } else if (entry?.contentRect) {
        width = entry.contentRect.width;
        height = entry.contentRect.height;
      }
      // Coalesce to one measurement per frame so drag events cannot spam
      // SIGWINCH and leave the grid oscillating between adjacent sizes.
      if (frame) cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        frame = 0;
        announce(width, height);
      });
    });
    observer.observe(element);
    return () => {
      if (frame) cancelAnimationFrame(frame);
      observer.disconnect();
    };
  }, [cell, fixedCols, fixedRows, measure, send]);

  const onKeyDown = useCallback(
    (event) => {
      const bytes = keyBytes(event, modesRef.current.application_cursor);
      if (!bytes) return;
      event.preventDefault();
      send({ type: "input", data: toBase64(bytes) });
    },
    [send],
  );

  // Mouse. When the program has not asked for tracking we do nothing at all,
  // so the browser's own selection and context menu keep working; that is the
  // behaviour a user expects from a terminal that is not in a mouse mode.
  const cellAt = useCallback(
    (event) => {
      const box = containerRef.current?.getBoundingClientRect();
      if (!box) return null;
      return {
        col: Math.max(0, Math.floor((event.clientX - box.left) / cell.width)),
        row: Math.max(0, Math.floor((event.clientY - box.top) / cell.height)),
      };
    },
    [cell],
  );

  const buttonName = (event) =>
    event.button === 1 ? "middle" : event.button === 2 ? "right" : "left";

  const sendMouse = useCallback(
    (type, event, button) => {
      const tracking = modesRef.current.mouse;
      if (!tracking || tracking === "none") return false;
      // Press-only tracking gets presses; a program that asked for nothing
      // more must not be sent releases or motion it cannot parse.
      if (tracking === "press" && type !== "down" && type !== "wheel") return false;
      if (type === "move" && tracking !== "button_motion" && tracking !== "any_motion") {
        return false;
      }
      const at = cellAt(event);
      if (!at) return false;
      event.preventDefault();
      send({
        type: "input",
        data: toBase64(
          mouseBytes({
            type,
            button,
            col: at.col,
            row: at.row,
            shift: event.shiftKey,
            alt: event.altKey,
            ctrl: event.ctrlKey,
            encoding: modesRef.current.mouse_encoding,
          }),
        ),
      });
      return true;
    },
    [cellAt, send],
  );

  const wheelRef = useRef(0);
  const draggingRef = useRef(null);
  const onMouseDown = useCallback(
    (event) => {
      const button = buttonName(event);
      if (sendMouse("down", event, button)) draggingRef.current = button;
      containerRef.current?.focus();
    },
    [sendMouse],
  );
  const onMouseUp = useCallback(
    (event) => {
      sendMouse("up", event, draggingRef.current ?? buttonName(event));
      draggingRef.current = null;
    },
    [sendMouse],
  );
  const onMouseMove = useCallback(
    (event) => {
      // Only drags are reported. Without knowing whether the program asked for
      // any-event tracking, sending every hover would flood a program that
      // only wanted button events.
      if (draggingRef.current) sendMouse("move", event, draggingRef.current);
    },
    [sendMouse],
  );
  const onWheel = useCallback(
    (event) => {
      const up = event.deltaY < 0;
      if (sendMouse("wheel", event, up ? "up" : "down")) return;
      event.preventDefault();

      // Accumulate pixels and emit one line per notch-third. A trackpad sends
      // many small deltas and a wheel sends few large ones; carrying the
      // remainder makes both scroll at the same rate rather than making the
      // wheel fly.
      wheelRef.current += event.deltaY;
      const step = 40;
      const lines = Math.trunc(wheelRef.current / step);
      if (lines === 0) return;
      wheelRef.current -= lines * step;

      if (modesRef.current.alternate_screen) {
        // A program owning the whole display has no scrollback to move
        // through, so the wheel becomes cursor keys instead. That is what
        // makes it scroll in vim and less.
        const key = lines < 0 ? "A" : "B";
        const prefix = modesRef.current.application_cursor ? "\x1bO" : "\x1b[";
        send({
          type: "input",
          data: toBase64(
            encoder.encode(`${prefix}${key}`.repeat(Math.min(Math.abs(lines), 10))),
          ),
        });
        return;
      }

      // Otherwise move through retained history. Both the new offset and the
      // request are derived inside the updater from the state React actually
      // holds: several wheel events can land before a render commits, and
      // reading a mirrored ref would lose all but the first.
      setScroll((previous) => {
        const screen = previous.screenRows || 1;
        // The furthest back worth going is the window minus one screen.
        // Clamping against the scrollback figure from the last reply instead
        // stalls the scroll at whatever that first reply happened to report.
        const limit = Math.max(0, previous.total - screen);
        const offset = Math.max(0, Math.min(limit, previous.offset - lines));
        if (offset === previous.offset) return previous;
        if (offset > 0) {
          send({
            type: "history",
            start: Math.max(0, previous.total - screen - offset),
            count: screen,
          });
        }
        return { ...previous, offset };
      });
    },
    [send, sendMouse],
  );
  const onContextMenu = useCallback((event) => {
    // A program tracking the mouse wants button 2, not the browser's menu.
    if (modesRef.current.mouse !== "none") event.preventDefault();
  }, []);

  const onCopy = useCallback((event) => {
    const selection = globalThis.getSelection?.()?.toString();
    if (!selection) return;
    // Normalize NBSP from layout so pasted shell commands stay plain ASCII.
    event.clipboardData?.setData("text/plain", selection.replace(/\u00a0/g, " "));
    event.preventDefault();
  }, []);

  const onPaste = useCallback(
    (event) => {
      event.preventDefault();
      const clipboard = event.clipboardData;
      if (!clipboard) return;

      const text = clipboard.getData("text/plain") || clipboard.getData("text");
      if (text) {
        // Bracketed paste lets the program tell a paste from typing, which is
        // what stops an editor auto-indenting pasted code.
        send({
          type: "input",
          data: pasteTextPayload(text, modesRef.current.bracketed_paste),
        });
        return;
      }

      const imageItem = [...(clipboard.items || [])].find((item) =>
        item.type.startsWith("image/"),
      );
      const imageFile =
        imageItem?.getAsFile?.() ||
        [...(clipboard.files || [])].find((file) => file.type.startsWith("image/"));

      if (imageFile) {
        const endpoint = pasteUrl || pasteEndpoint(url);
        void (async () => {
          try {
            const response = await fetch(endpoint, {
              method: "POST",
              headers: {
                "content-type": imageFile.type || "image/png",
                "content-disposition": `attachment; filename="${(imageFile.name || "clipboard.png").replace(/"/g, "")}"`,
              },
              body: imageFile,
            });
            if (!response.ok) return;
            const payload = await response.json();
            if (!payload?.path) return;
            send({
              type: "input",
              data: pasteTextPayload(
                shellPath(payload.path),
                modesRef.current.bracketed_paste,
              ),
            });
          } catch {
            // Paste is best-effort; a failed upload must not take the session down.
          }
        })();
      }
    },
    [pasteUrl, send, url],
  );

  const background = cssColor(palette?.background, "#101014");
  const foreground = cssColor(palette?.foreground, "#e6e6e6");
  const cursorColor = cssColor(palette?.cursor, foreground);

  return h(
    "div",
    {
      ref: containerRef,
      tabIndex: 0,
      onKeyDown,
      onCopy,
      onPaste,
      onMouseDown,
      onMouseUp,
      onMouseMove,
      onWheel,
      onContextMenu,
      style: {
        position: "relative",
        width: "100%",
        height: "100%",
        overflow: "hidden",
        background,
        color: foreground,
        fontFamily,
        fontSize: `${fontSize}px`,
        lineHeight: 1.2,
        whiteSpace: "pre",
        outline: "none",
        cursor: "text",
        userSelect: "text",
        WebkitUserSelect: "text",
      },
      "data-status": status,
    },
    // While scrolled back the history rows replace the live screen. The live
    // grid is left untouched underneath, so releasing the scroll shows exactly
    // what arrived meanwhile rather than a reconstruction.
    (scroll.offset > 0 && scroll.rows
      ? Array.from({ length: grid.rows }, (_, index) => {
          const absolute = (scroll.start ?? 0) + index;
          return scroll.rows.get(absolute) ?? [];
        })
      : grid.lines
    ).map((cells, y) =>
      h(
        "div",
        { key: y, style: { height: `${cell.height}px` } },
        cells.length ? runs(cells, palette, stylesRef.current, cell.width) : " ",
      ),
    ),
    scroll.offset > 0 &&
      h(
        "div",
        {
          style: {
            position: "absolute",
            right: "8px",
            top: "4px",
            padding: "2px 8px",
            borderRadius: "4px",
            background: "rgba(0,0,0,0.6)",
            color: "#cfcfd6",
            fontSize: "11px",
            pointerEvents: "none",
          },
        },
        `${scroll.offset} rows back`,
      ),
    scroll.offset === 0 && cursor.visible &&
      h("div", {
        style: {
          position: "absolute",
          left: `${cursor.col * cell.width}px`,
          top: `${cursor.row * cell.height}px`,
          width: `${cell.width}px`,
          height: `${cell.height}px`,
          background: cursorColor,
          opacity: 0.7,
          pointerEvents: "none",
        },
      }),
  );
}

export default Terminal;
