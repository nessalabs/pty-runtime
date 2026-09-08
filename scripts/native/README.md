# Pinned Ghostty adapter build and proof

The `ghostty` infrastructure feature compiles the C bridge against the pinned
`82232ecde55405559dec29c5466cb9e39938cb41` headers and links the real static VT
library. `build.rs` verifies the cached source archive SHA-256 and matches
extracted native source files against the archive, except for the exact reviewed
[snapshot cursor correction](patches/README.md). The existing bootstrap applies
that hash-verified correction before building; Cargo verifies its build stamp
and library digest before linking. It requires the existing
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

## READY, live mutation, and history accounting

The pinned engine permits feed/resize between history steps. Some source pages
become inapplicable after width changes, screen replacement or history-budget
overflow. The adapter now advertises `mutation_during_restore: true`, reports
validated-but-skipped pages explicitly, and distinguishes full history completion
from a finished source with skipped history. Observing READY alone still does
not certify source integrity or full history.

The coordinator alternates one admitted live operation with one bounded history
step. It retains the saved source and restoration memory until FINISH validates
the source. Output and resize remain in their original order. Transfer End also
waits for this validation; a later corrupt source cannot follow a successful End.
Session status preserves cumulative omitted-page accounting across later parks.

The earlier active-screen comparison missed history loss. Current real-native
regressions use the complete canonical-state oracle: 100,000 source lines and
interleaved same-width live output with spare history quota match an uninterrupted
reference exactly. Separate width-change and quota tests require explicit skipped
history outcomes. A deterministic real-Ghostty/AEAD/disk/coordinator test confirms
live output at READY and retained ciphertext before history FINISH; its process
events are injected. Actual PTY projection tests remain a separate evidence scope.

These adapter tests do not establish integrated capacity, supported-platform
release performance, fault-injection completeness or the twelve-hour soak.
