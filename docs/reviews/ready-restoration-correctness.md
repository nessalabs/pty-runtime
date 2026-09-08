# Independent READY restoration correctness review

Reviewed 2026-09-08 on macOS arm64 by the correctness specialist, independently
of the implementation author. No open P1/P2 findings in this change's scope.
The final frozen-source repository gate and other specialist reviews remain
separate requirements, owned by the coordinating reviewer.

## Scope and findings

Inspected the pinned native decoder's `Progress.rows` contract and `nextPage`
validation paths, the C/Rust conversion, coordinator FIFO/history alternation,
restore source and plaintext reservations, failure retention, End gating, and
per-source/cumulative domain omission counts. Zero rows denotes a validated but
unapplied history page; source FINISH remains mandatory. Successful source
consumption with omissions is separately represented from complete restoration.
An admitted output or resize alternates with a history step; queued checkpoints
wait for source validation. Saved ciphertext and restoration memory survive until
FINISH, and native history failure retains the source while failing projection.
The stream cannot publish successful End while restoration remains in progress.

Added an independent regression to `terminal_live_restore.rs`: truncate only the
source's final byte, restore READY, resize to make history inapplicable, observe
an actual skipped page, then consume the source. The adapter must report
CorruptCheckpoint, never claim finished history, and reject later feed/view.
This passed and closes the otherwise untested combination of live skip and
late source corruption; no production fix was required.

## Independent execution

- Three application READY orchestration tests passed.
- Six real Ghostty terminal contract tests passed, including full canonical
  comparison of 100,000 history lines and live feed.
- Three real Ghostty live restoration tests passed, including the new regression.
- The real Ghostty/AEAD/disk/coordinator 8192+64-byte READY test passed. Its process
  events are injected; this is not an actual PTY workload claim.
- Infrastructure strict Clippy with Ghostty and all targets passed.

Commands: `cargo test --locked -p pty-runtime-application projection::tests::ready
--lib`; `cargo test --locked -p pty-runtime-infrastructure --features ghostty
--test terminal_live_restore --test terminal_contract`; `cargo test --locked
--features ghostty --test ready_projection`; `cargo clippy --locked -p
pty-runtime-infrastructure --features ghostty --all-targets -- -D warnings`.

This review does not establish unsupported-platform native behavior, aggregate
release capacity/performance, all fault combinations, or the twelve-hour soak.
