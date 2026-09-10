# Workspace-sdk integration sketch: independent DDD review

Reviewed 2026-09-08. Scope: `docs/examples/workspace-sdk-integration.md`, the additions to `docs/usage.md`, and read-only comparison with the nine sibling sources selected in `docs/verification/workspace-sdk-sketch/source.json`. Applied this repository's AGENTS.md, coding standards and ADR 0005; checked ADR 0001 section 7. Read the sibling AGENTS.md; no sibling files were changed.

## Result

No P1/P2 DDD or boundary blocker found. The sketch is suitable as a proposed adapter design. It does not establish an implemented SDK transport, deployed target owner, tested SDK integration, or release readiness.

## Evidence and findings

- Target ownership is explicit in the sketch's “Current boundary and required host lifetime” section. The sibling `runtime/backend_host.rs:239-248` rejects terminal-required interactive starts; `ports/i_backend.rs` exposes ordinary execution/start/output/cancel but no terminal start/input/resize. `providers/managed/managed_backend.rs:79-83` invokes a typed Start request through the target. `supervisor/cli.rs:16-23,49-56` and `bin/workspace-supervisor.rs:3-6` implement a one-request dedicated executable entry point, with launch-specific detachment rather than a reusable multithreaded PTY service endpoint. Requiring a separate long-lived target-side runtime and reconnecting short-lived transport invocations is the correct prospective ownership distinction. The document does not substitute a client-local runtime for a remote workspace runtime or imply the present CLI already provides that owner.
- The new boundary is an adapter/transport extension in the sibling package, not an import of SDK implementation or serialization into pty-runtime domain/application. Existing execution, credentials provisioning, inspection and status verification stay on their current paths. Unsupported terminal capability stays explicit. Raw PTY transcript use is separated from optional terminal projection.
- Session identity maps to the SDK's scoped opaque reference and explicitly binds the runtime lifetime. This matches `runtime/domain/runtime_process.rs`, whose persisted reference has host_scope and id but no separate generation. A bounded registry or opaque encoding must supply the missing lifetime binding; stale references cannot target reused IDs. Same-key concurrency and changed-command reconciliation are stated as future adapter obligations, not falsely attributed to bare `Runtime::lookup`.
- Portable state and error conversion retains uncertainty. Known code exits, signals requiring a defined representation, Lost on uncertain supervision/owner loss, separate authentication-status verification, and cancellation admission versus observed exit are all distinguished. The SDK's `RuntimeProcessState` has only Running/Exited/Lost, so documenting the signal representation as follow-up is appropriate rather than leaking native wait status or inventing an existing variant.
- Input handling follows `IRuntimeHost` and `RuntimeInputOutcome`: a proven accepted prefix differs from a lost acknowledgement; cancellation of a wait cannot authorize replay. The sketch requires finite compatible chunk limits and does not assert OS writes prove child consumption. Projection resize retains separate model/OS outcomes. These conversion rules preserve delivery certainty despite the SDK's narrower outcome model.
- Output conversion respects SDK raw-byte limits and cursors, merged stdout, zero stderr cursor, explicit gaps, lossy decoding expansion and runtime lifetime validation. The prefix-page rule avoids advancing the externally acknowledged cursor past returned bytes. Confirmed completion cannot conceal failed/truncated drain. The sketch does not conflate persisted SDK receipts with durable PTY process recovery.
- `docs/usage.md` labels this as proposed sibling integration. Its reader-gauge addition states actual scratch capacity and exclusions, consistent with the separately reviewed diagnostic implementation. No new runtime capability is claimed there.

## Verification and remaining work

Independently verified all nine sibling SHA-256 values in source.json against the current files and confirmed the sibling directory has no .git entry. This is a selected-source snapshot, not an inferred sibling revision. No tests, builds, source edits, deployment, native investigation or SDK integration execution were performed. The future integration's concurrency, reference/lifetime, partial-input, output-boundary, resize/cancel, disconnect and owner-loss tests remain required as the sketch states. This review does not replace the coordinating agent's documentation/mechanical checks or release proof ledger.

## Reviewed artifact identity

| Artifact | SHA-256 |
| --- | --- |
| `docs/examples/workspace-sdk-integration.md` | `3296237a6b76cc04ea570499cc7620518fd460ac7048b970bf5012760e4eabe1` |
| `docs/usage.md` | `4d021fcf1ff6f35aced427081e7151aafc62ea23038c0c3e936f8f76b26613ef` |
| `docs/verification/workspace-sdk-sketch/source.json` | `4ece2ceac20f0aa9deded3c83fa2ad73d49bcb81e30e2a0c3f44f2dd1f49503e` |
