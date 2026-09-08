# Independent event-stream architecture review

Date: 2026-09-08. Reviewer: scheduler_adapter. Independent scope is root-authored `crates/infrastructure/src/event_stream/{publisher,codec,status,errors,mod}.rs`, feature exports, and manifests. Read coding_standards.md and ADR 0001's Event-stream contract. This review does not self-review the ordered-transfer implementation authored by this reviewer. No implementation files were edited for this review.

## Findings and resolution

**P2 — Receipt validation allowed a repeated or regressing store cursor (independently resolved).** Originally publisher receipt validation checked payload/identity/schema equality, stream incarnation, version, and nonzero offset, but did not compare with the previous successful receipt. An injected sink could therefore acknowledge a distinct later pending event at an already-acknowledged or earlier store offset. The publisher would advance its PTY acknowledgement even though the returned reconnect position contradicted sequential append order. Interleaved writers justify gaps in store offsets, not regressions. Root added `last_event_offset`, rejects `receipt.record.cursor.offset <= self.last_event_offset` (`publisher.rs:116-121`), and changes that field only with a fully validated receipt (`publisher.rs:128-131`). Pending state survives rejection. Independently inspected the fix and ran `contradictory_store_cursor_cannot_acknowledge_a_different_completion_event`; it rejects the contradictory completion receipt, preserves pending identity, then successfully reconciles the same event at the correct offset. The explicit `<=` predicate also rejects decreasing offsets.

A preliminary diagnostic concern was withdrawn after source inspection: `DecodedOutput`'s derived Debug delegates to `ReplayPage`'s manual Debug in `crates/domain/src/replay.rs:43-59`, which prints cursor/length and never byte contents. Another custom wrapper Debug is unnecessary. This is not an unresolved finding.

No other P1/P2 architecture or ownership defect is confirmed in the reviewed scope.

## DDD and dependency direction

The optional, revision-pinned event-stream dependency exists only in infrastructure. Domain has no dependencies and application depends only on domain. Infrastructure consumes a concrete quota-counted application Attachment and converts its typed output/gap/completion values to an external schema. The public facade re-exports this explicit optional adapter. This is the appropriate location for dependency-specific forwarding; introducing a generic application event-store port merely to mirror the external API would not improve the boundary.

The adapter is caller-driven. Construction creates no stream, store, persistence facility, task or worker. A caller supplies the resolved stream incarnation, sink and executor. The adapter never interprets an external event-store offset as a PTY byte position. `Publication` exposes those two cursors separately, and codec records contain original lifetime-bound byte positions. Stream subscription/reconnect and external retention stay with the pinned dependency.

Portable completion/error serialization remains in infrastructure's explicit versioned codec and stable status-code mapping. Domain enums acquire no serialization attributes or foreign transport types. Unknown schema/version/status codes, truncated or trailing completion fields, and impossible byte range lengths reject with a redacted typed adapter error.

## Independent Clean Code / SOLID / ownership assessment

The focused modules have distinct responsibilities: publisher state and acknowledgement, binary conversion, stable error-code conversion, redacted sink errors, and exports. `Pending::{Source,Prepared}` makes the key ownership transition explicit. Source is installed immediately after the raw attachment advances and before fallible conversion (`publisher.rs:102-108`). Encoding or identifier construction failure therefore retains the same source rather than reading past it. After preparation, repeated or cancelled appends reuse the same immutable ID/schema/payload; only a validated acknowledgement clears pending state. This is a suitable small concrete state machine, without a service locator or an unnecessary interface hierarchy.

Resource ownership is finite and documented. Each publisher consumes an already-admitted global/per-session attachment slot and retains at most one source page. Runtime policy caps the page at 65,536 bytes. Encoding simultaneously owns that page, one bounded Vec, and one immutable external Payload copy; after preparation only the Payload remains. The pinned dependency's `Payload` is `Arc<[u8]>` (`event-stream` revision 66ba752, `src/domain/model.rs:61-76`), so `NewEvent::clone` during retry shares the bytes rather than making another payload copy. ID/schema strings are bounded independently. External sink queue and retention ownership are explicitly external and are not misrepresented as publisher-owned memory. No sink await holds a PTY/model/runtime lock; a slow store advances no source reads beyond its one pending event and cannot block reader drain.

The API documents cancellation, uncertain commits, publisher drop, new-publisher identity, and the deliberate lack of cross-publisher deduplication. Errors omit external diagnostic text and payloads. No unneeded generic repository, mirrored domain DTO layer, or resource ownership cycle was found.

## Verification and limits

Independently ran `cargo test --features event-stream --test event_stream_failures --test event_stream_reconnect`: all four tests pass. This covers actual pinned-store publication/reconnect with interleaved writers and preserved gaps, controlled rejection/uncertain commit/invalid receipts, cancellation before and after commit, and the new contradictory-offset regression. Source-retention on encoding failure was verified structurally; no allocator failure injection is claimed.

The run reported one unused test import in `tests/support/event_store.rs`; root was notified so its final formatting/Clippy gate can resolve it. No full gate was run concurrently with the coordinated review. The final mechanical gate, performance qualification, and broader milestone claims remain root-owned.

Result: no unresolved P1/P2 findings in this independent scope. The receipt-ordering P2 is independently re-reviewed as resolved.
