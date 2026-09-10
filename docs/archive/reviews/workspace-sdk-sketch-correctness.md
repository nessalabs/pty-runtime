# Workspace SDK integration sketch correctness review

No P1/P2 correctness finding in this documentation-only sketch. It accurately distinguishes inspected sibling behavior, proposed adapter requirements and completed library work. Review was read-only for both repositories; no sibling/source edit, implementation, test or workload was performed.

All nine sibling-source hashes in `docs/verification/workspace-sdk-sketch/source.json` match the inspected files exactly. `/Users/nessa/Documents/NessaLabs/workspace-sdk/.git` is absent; the null sibling revision is appropriate. `BackendRuntimeHost::start_interactive` rejects terminal-required starts; `IRuntimeHost` declares the relevant operations but the current backend port does not provide terminal start/input/resize. Managed operations use typed supervisor requests through the transport. `run_cli` explicitly handles one request in a dedicated single-threaded process, so the proposed long-lived target-side PTY owner is necessary rather than claiming the current transport already supports it.

The SDK process reference preserves exact host scope and is an opaque identity, not a PID. The sketch adds the necessary session-lifetime binding and rejects stale references/changed attempts. Its same-key admission coordination is an adapter requirement, not a promise that `Runtime::lookup` itself blocks for an in-progress spawn or validates command equality. Runtime shutdown owns process termination; dropping an attachment, session handle or transport future does not imply shutdown.

Replay mapping is correct: SDK cursors count original raw bytes, merged PTY output goes to stdout, stderr remains zero, and gaps advance absolute positions. For an attachment page beginning at offset x and an SDK limit n smaller than the page, returning x+n avoids falsely acknowledging its unread suffix; the next request must reattach at that returned cursor. `try_next` advances its private cursor across the whole page, so using that cursor after truncation would be wrong; the sketch explicitly rejects that mistake. Lossy UTF-8 output size is correctly separated from raw cursor accounting, matching the SDK's documented expansion/split-character behavior. A pending/gap-only response can be empty and incomplete. Completion requires consumed output and actual completion, with failed/truncated drain still explicit.

Input mapping preserves delivery certainty. Runtime input admission returns a pending `WriteOutcome`; dropping its wait does not revoke or resend the accepted work. `written` is the OS-accepted prefix, not proof of child consumption. Reporting that prefix as SDK `Accepted { bytes }` and ambiguous transport acknowledgement as `Unknown` preserves the SDK rule against blindly replaying the whole submission. The sketch limits ordinary errors to established zero-admission cases, requires bounded chunking or compatible finite configuration for larger SDK inputs, and keeps secrets out of logs/persistence. The inspected SDK `SecretInput` is a byte vector without the runtime's default 4,096-byte chunk limit, so the compatibility warning is warranted.

Resize correctly distinguishes raw operation completion from the projected OS/model outcomes. Cancellation remains an admission followed by observed state; a timeout does not fabricate an exit. Known exits, supervision/admission uncertainty and owner loss map without claiming authentication success from exit zero. The document correctly says model-dependent encoding is a future API requirement and does not invent a public key/mouse encoder or completed parked-resize proof.

The usage link identifies this as a proposed sibling transport, and the adjacent reader-gauge wording retains the settled-reader/allocator-and-stack limitations reviewed separately. The explicit follow-up tests are appropriate integration obligations; none is presented as already executed against workspace-sdk.

## Reviewed identities

| File | SHA-256 |
|---|---|
| `docs/examples/workspace-sdk-integration.md` | `3296237a6b76cc04ea570499cc7620518fd460ac7048b970bf5012760e4eabe1` |
| `docs/usage.md` | `4d021fcf1ff6f35aced427081e7151aafc62ea23038c0c3e936f8f76b26613ef` |
| `docs/verification/workspace-sdk-sketch/source.json` | `4ece2ceac20f0aa9deded3c83fa2ad73d49bcb81e30e2a0c3f44f2dd1f49503e` |
| `crates/application/src/runtime/attachment.rs` | `137c5b17e68ed3a71120c9f640731076d37c514e9866b870fce7716475c806d0` |
| `crates/application/src/runtime/context.rs` | `eae611b862fd895790573ed8fce077c937d8e8c14e44d60c87098e526729390c` |
| `crates/application/src/runtime/session.rs` | `42ea83242374a3e0c7c317f62033dd57ef7324d61f82a88c292c175c614d37df` |
| `crates/domain/src/process/outcome.rs` | `82f7b9cdeb72ff512ff9e4c15b2ac248c4f5266d250ddd850139faea59406874` |
| Additional read-only SDK source: `src/domain/execution/command.rs` | `9cdd4b818b265a49a07bfa68b73249ee819c7ec3602faae6c657f0e085781776` |
