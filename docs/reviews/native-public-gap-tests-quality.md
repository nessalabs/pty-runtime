# Native public-gap tests: independent quality review

**Scoped PASS; no unresolved P1/P2 finding.** Reviewed the actual changes in
terminal_contract.rs, terminal_bounds.rs and process_image_materialize.rs,
plus saved execution logs and the three-file source manifest. All three current
hashes match the retained test bindings. No build or benchmark was run by this
reviewer; full mechanical/coverage/performance and separate adversarial gates
remain required.

The VT tests use the real Ghostty factory and externally defined escape
sequences. Exact text, width and underline enum assertions check conversion
semantics; retaining the first six complete cells after subsequent output checks
that reset affects later text without rewriting earlier decorations. The later
slices/index assertions also prevent the initial zipped comparison from passing
vacuously on a short cell array. X10 enable/disable checks Press/None independently
of SGR encoding while preserving full cells and cursor, making this a public
mode contract rather than an implementation-branch exercise.

Restore tests begin with a valid factory-produced checkpoint. They distinguish
one-byte-too-small from exactly sufficient budgets and wrong from correct
terminal dimensions, require precise typed errors, and then restore successfully
and compare continued feeds. No malformed private native state or replacement
serializer oracle is introduced. The existing focused fixtures keep composition
simple and assertions local; no shared testing framework is warranted.

The materializer tests call the existing owned-write entry point with real
filesystem conditions. Permission denial must preserve an existing sentinel,
leave the destination absent, preserve the caller's signal mask and allow a
valid retry once permissions are restored. The embedded-NUL case proves exact
InvalidCommand without truncating to the existing prefix or adding a directory
entry, then succeeds on a valid path. Cleanup restores directory permissions on
Drop before removal, including failed assertions. These tests add no fork hook,
private poison state, native failure switch or production behavior change.

The EACCES test requires ordinary unprivileged permission enforcement. Its saved
run used uid 501; it is not evidence of the same result under root or capability
bypass. The test deliberately fails if the expected permission denial does not
occur rather than silently claiming that path was exercised. This operational
assumption is disclosed in the author rationale and remains a portability limit.

Saved terminal.log records all seven terminal_bounds and five terminal_contract
tests passing. Saved materializer.log records all six materializer cases passing,
including both additions. The author reports formatting passed. These are
contract/regression results on the stated source identity, not a measured final
coverage gain or completion of the native release/performance requirements.

Reviewed source bindings:

- `crates/infrastructure/tests/terminal_contract.rs`: `e1b03140720c30f79f90d58016ed9aac64fe1e53e8d730f137c32926ad52928d`
- `crates/infrastructure/tests/terminal_bounds.rs`: `9078c83784ad3f4a73622dddef48f64156cee481574934e97324d06151b91497`
- `crates/infrastructure/tests/fixtures/process_image_materialize.rs`: `208d39404e40dc32dd2b18062def95ba6ac6cc2889fa2b057f6da82fd3959a0a`
