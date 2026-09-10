# Interactive example adversarial correctness review

Review date: 2026-09-08. Scope: `examples/interactive.rs`,
`examples/interactive_support/host.rs`, `scripts/release/interactive_smoke.py`,
and `docs/examples/interactive.md`. Independent review of the preceding agent's
implementation and cancellation fix; no production runtime changes in this review.
Source fingerprints and executed evidence: `docs/verification/interactive/`.

## Findings and re-review

- **P1 resolved in scoped re-review:** A pending input write prevented reading
  Ctrl-] (`interactive.rs:62-89`). The earlier reproduced failure is preserved in
  `docs/verification/interactive-review/stalled-input.jsonl`: 3066 input bytes,
  no accepted escape byte, then an input timeout. The host now preserves ISIG,
  binds VINTR to Ctrl-], disables other signal characters, and uses NOFLSH
  (`host.rs:64-78`). The signal is inspected before pending operations on every
  iteration (`interactive.rs:58-60`); expected operation errors after cancellation
  do not replace the cancellation outcome (`interactive.rs:65,77`). The independent
  real controlling-terminal smoke passes on macOS and Linux for escape cancellation with a nonreading
  child, below its two-second bound, and verifies restoration/reaping. This does
  not establish arbitrary-volume paste responsiveness: the fixture submits a
  bounded 2000-byte Darwin / 6000-byte Linux load, and does not instrument runtime
  write admission to prove exactly when a write becomes pending.
- **P2 resolved in scoped re-review:** An outer PTY without a controlling session
  could not faithfully test VINTR delivery. The client now requires foreground
  ownership (`host.rs:31-36`); the smoke arranges a session/controlling terminal and
  keeps its leader alive through inspection. That is necessary on Darwin, which
  revokes the slave when the session leader exits. The host wrapper ignores group
  SIGINT while the client handles it, and forwards external TERM to the client.
- **P3 observation:** The smoke verifies forwarded Ctrl-Z by a child signal handler;
  it proves delivery, not stop/continue orchestration. The documented example does
  not claim to suspend/resume the outer client. The raw terminal renderer bypasses
  Ghostty and cannot qualify terminal-model compatibility.

Resource/ownership inspection: one 4096-byte input chunk, one pending write, one
pending resize and one bounded replay page; nonblocking host descriptors; ten-second
cancellation watchdog; terminal guard outlives the runtime. Signal handlers only
store an atomic integer. Both Rust source files remain below 350 nonblank lines.
The new smoke timing fields are diagnostic observations, not release benchmarks.

No new demonstrated P1/P2 issue remains in this bounded example review. Full
repository gate and DDD/design specialist acceptance are coordinated by the root
review; this document does not claim the repository milestone passed.
