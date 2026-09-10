# Resumed helper image DDD review

Reviewed 2026-09-08 against HEAD `46dbec64384e4519e751086ef47feb5bac11024d`
plus the current working tree. Independent source review; no production edits.
Read `AGENTS.md`, `coding_standards.md`, and ADR 0005.

## Scope and result

No P1/P2 DDD or boundary-contract blockers found in the inspected scope:
the production diff in `process/backend.rs`, `process/image.rs`, and
`process/mod.rs`, plus the untracked `process/image_materialize.rs` and its
test-only `image_materialize_hook.rs`. Inspected the process error model,
backend construction sequence, and both image fixture files as supporting context.
The release census change was inspected but is outside this specialist's
architecture acceptance scope; projection test additions do not alter production
dependency direction.

- OS process creation, masks, filesystem paths, descriptors, errno, and native
  wait statuses remain inside infrastructure. The new module is private
  (`process/mod.rs:6`), and its entry point returns the existing domain error
  (`image_materialize.rs:14`). No application/domain dependency edge is added.
- The child's small status protocol stays local to the adapter:
  `image_materialize.rs:173-177` converts errno to child status, and
  `image_materialize.rs:113-120` converts termination/status to stable
  `ProcessError` variants. Raw status, PID, path text, and errno do not leak into
  core types or errors. Existing destination remains `Io`, matching the existing
  general filesystem conversion contract.
- `HelperImage` retains staging selection and cleanup ownership
  (`image.rs:50-83`); the extracted materializer owns write/reap mechanics.
  The image is returned only after materialization succeeds. A new application
  port for this internal implementation detail would not improve dependency
  inversion.
- The public adapter documents the added constructor child, platform atfork
  behavior, and unbounded OS wait (`backend.rs:38-43`). Host validation still
  precedes materialization (`backend.rs:87-92`). These implementation-specific
  lifecycle constraints stay on the Unix adapter rather than becoming domain
  policy.
- Test synchronization is compiled only for tests and executes the observer in
  the parent. It adds no public contract or production callback dependency.

## Verification limits

This is a source architecture review, not a passed milestone. No gate, build,
runtime test, performance run, or coverage measurement was run by this reviewer;
the coordinating agent owns the review-loop gate and evidence. Concurrency,
platform atfork safety, fault-injection completeness, and full release readiness
require their separate correctness review and executable evidence.
