# Independent organization review: workspace-sdk sketch

No P1/P2 organization/cohesion finding. The document satisfies the requested **adapter sketch** scope of ADR 0001 section7; it does not claim sibling integration was implemented or executed. No code, sibling package, build or workload was changed by this reviewer.

The proposed target is concrete: adapt `BackendRuntimeHost::start_interactive`/existing `IRuntimeHost` operations through a long-lived target-side PTY owner, while existing short-lived managed transport/supervisor invocations reconnect to that owner. It identifies the missing terminal operations in `IBackend` and makes deployment/transport extension subsequent sibling work. Local ownership is explicitly prevented from standing in for a remote target. Existing noninteractive, private-file and authentication-status paths remain their current responsibilities; no unrelated feature is proposed as part of this library deliverable.

The organization follows the decision dependency: current host lifetime and missing transport capability, a bounded operation-mapping table, the two material translation limits (raw byte cursors/page prefixes and partial/unknown input), then applicability and follow-up proof. Lifetime-bound references, immutable attempt parameters and no-respawn reconnect are coherent. The table does not hide incomplete mappings: signal exit, ambiguous input acknowledgement, projected OS/model resize outcomes, failed/truncated drain and owner loss require explicit SDK representations or policy. The already-encoded-input explanation avoids inventing a model-dependent input API.

The usage addition points to the detailed sketch and clearly labels it proposed. Reader-gauge wording in the same usage diff preserves capacity-versus-stack/allocator distinctions and requires settled observation; it does not expand the integration claim.

Source binding was independently checked: all nine selected sibling files still match `docs/verification/workspace-sdk-sketch/source.json`; the sibling has no asserted Git revision. Relevant named host/backend entry points and ADR section7 were read. Exact reviewed document/evidence hashes are retained below. This is organization and scope review, not execution of the future SDK adapter or a complete behavioral integration verdict. The follow-up proof list correctly remains pending.

| Reviewed artifact | SHA-256 |
|---|---|
| `docs/examples/workspace-sdk-integration.md` | `3296237a6b76cc04ea570499cc7620518fd460ac7048b970bf5012760e4eabe1` |
| `docs/usage.md` | `4d021fcf1ff6f35aced427081e7151aafc62ea23038c0c3e936f8f76b26613ef` |
| `docs/verification/workspace-sdk-sketch/source.json` | `4ece2ceac20f0aa9deded3c83fa2ad73d49bcb81e30e2a0c3f44f2dd1f49503e` |
| `docs/adr/0001-pty-runtime.md` | `8acb7f7b01c8f5d669f2bf99fc23cb85b97c467840ad14596a8ed60c71f31a15` |
