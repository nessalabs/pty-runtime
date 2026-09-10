# Client / demo terminal

## Intent

A thin React viewer paints grids the runtime already laid out. A small demo
server owns one session and speaks `client/PROTOCOL.md`. Image paste saves under
`$TMPDIR/pty-runtime-paste/` and inserts the file path. Local demo only.

## Code

- `client/web/terminal.js`
- `client/server/`
- `client/PROTOCOL.md`
- [`../examples/interactive.md`](../examples/interactive.md)

## Status

See [`../verification.md`](../verification.md). Try the demo locally.
