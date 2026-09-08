# Pinned Ghostty adapter build and proof

The `ghostty` infrastructure feature compiles the C bridge against the pinned
`82232ecde55405559dec29c5466cb9e39938cb41` headers and links the real static VT
library. `build.rs` verifies the cached source archive SHA-256 and matches
extracted native source files against the archive. It requires the existing
experiment bootstrap's compiled `zig-out/lib/libghostty-vt.a`; there is no stub
or automatic replacement. `PTY_RUNTIME_GHOSTTY_SOURCE` can select another cache
path containing the same verified archive and source. The compiled archive is
staged under a unique basename so Apple's linker cannot select the adjacent
Ghostty dynamic library instead.

The C bridge owns one allocator, terminal, and optional decoder. Its allocator
rejects requested native bytes above `TerminalConfig::native_bytes`; this cap
excludes libc allocation overhead and the small bridge owner. History has an
additional page-granularity native target. Rust separately admits feed, reply,
checkpoint, text-copy, and cell-count bounds. Kitty images and external image
media are disabled. All mutable native operations require the exclusive Rust
owner; moving the owner transfers its heap-stable callbacks/allocator together.
The C callbacks do not call Rust, allocate replies, or reenter the terminal.

## Verification commands

```
cargo test -p pty-runtime-infrastructure --features ghostty --test terminal_contract --test terminal_bounds
cargo clippy -p pty-runtime-infrastructure --features ghostty --all-targets -- -D warnings
python3 scripts/gate.py
```

On macOS arm64, the first two commands passed with 11 actual native tests on
2026-09-08. The repository gate and independent specialist review still apply
before a reviewed milestone can be claimed. These tests cover domain conversion,
split UTF-8/CSI/OSC/DCS/alternate-screen continuation, cursor/style/modes, replies,
ordered resize, bounds/failure categories, compression, wide and combining
characters, thread transfer, and decoder cleanup before history completion.

Checkpoint tests compare immediate native binary roundtrips exactly. After
mutation, native page layouts may legitimately differ: `verification.c` is an
independent native decode/formatter oracle comparing the complete formatted VT
state, including retained history, palette, modes, cursor/style, hyperlinks,
protection, keyboard state, charsets, tab stops, working directory and scrolling
region. The large reference retains and counts 100,000 history lines. Truncated,
corrupt, and appended transport bytes must fail before full completion; observing
READY alone never certifies checkpoint integrity or complete history.

## Verified restoration limitation

An initial reference test rendered identical active screens after feeding live
output at READY, but the full-state oracle found only 8,141 formatted bytes in
the restored model versus 14,642 in the uninterrupted reference. Fencing resize
alone did not fix this; live feed before history completion also lost history
in that test. The adapter therefore advertises `mutation_during_restore: false`
and rejects feed/resize with `HistoryIncomplete` before any mutation. READY
allows active-screen observation; incremental history steps continue. The
application must stage admitted output and controls under its own bounds and
apply them once, in order, after `Complete`. This preserves complete history;
it does not silently accept upstream's permission to skip inapplicable pages.

Native parking orchestration, encrypted storage, full PTY/reply integration,
Linux qualification, integrated memory/performance and release soaks are not
proved by this adapter-only suite.
