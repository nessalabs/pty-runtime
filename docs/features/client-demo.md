# Client / demo terminal

**ADRs:** embedding in [usage.md](../usage.md); scrollback consumer notes in
[0006](../adr/0006-scrollback-projection.md)

## Intent

A thin React viewer paints grids the runtime already laid out. A small Axum
demo server owns one `Runtime` session and speaks `client/PROTOCOL.md`.
Clipboard paste of images uploads to `$TMPDIR/pty-runtime-paste/` and inserts
the absolute path. This is a local demo, not a multi-tenant product surface.

## Code

- `client/web/terminal.js` — React component
- `client/server/` — WebSocket + `/paste-file`
- `client/PROTOCOL.md` — wire contract
- Examples: [`../examples/interactive.md`](../examples/interactive.md)

## Verification

See [verification index → client](../verification/README.md#client--demo).
Key folders: `interactive/`, `interactive-review/`.
