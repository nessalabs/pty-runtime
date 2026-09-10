# Independent EOF fixture and image mutation review

Reviewed 2026-09-08. No P1/P2 blockers found in this scoped source/evidence review.
No production edits, builds, or test runs by this reviewer.

## Protocol fixture

The diff in `scripts/guardian/protocol.rs` changes only `cfg(test)` code. Its
current SHA-256 is `2a1a51213f3455009ffab91159eebb039d46146c7ff900265418c858cef794b5`,
matching `docs/verification/resumed/protocol-eof-fixture/source.json`.

`await_eof` (lines 219-232) waits at most five seconds for observable closure,
rejects unexpected complete frames, and propagates errors. The partial-frame
cases require `InvalidData`; clean EOF cannot satisfy `unwrap_err`, unrelated
errors cannot satisfy the error-kind assertion, and a timeout fails the test.
The reset fixture (lines 303-319) holds a cloned peer, explicitly verifies pending
reads with `eof == false`, and then closes the last local peer owner. It leaves
the queued Release unread, preserving the real socket close/reset scenario.
The full-frame case still verifies the exact Retiring frame before clean EOF.

The original gate log records `unwrap_err()` receiving `Ok(None)`, not a recorded
clean EOF. Production `receive` permits this result for WouldBlock/Interrupted.
The correction therefore addresses a premature fixture assertion without relaxing
the truncation contract. The cloned descriptor demonstrates delayed closure; it
does not establish which descriptor caused the original concurrent occurrence.

Reviewed raw macOS logs: five targeted protocol tests and all 17 raw-only
infrastructure library tests pass. These support the fixture report accurately.
Linux execution of this particular test-only correction and the complete gate
remain outside the reviewed evidence. The old protocol fixture failure must not
be relabeled as production RED.

## Linux unchanged-hook image RED/GREEN

Inspected `docs/verification/resumed/linux-image-{green,mutation}` raw logs,
metadata source maps, and saved pristine/mutant source. The source-map comparison
has exactly one changed file: `process/image_materialize.rs`. The hook and
constructor fixture hashes are identical across both runs and match current
local files. The pristine materializer hash also matches current source:
`2f3d02bcc547feabed0d65398abe34ccbd37381ae6c71696cfa75e0e02917604`.

The mutation removes child fork/reap ownership and invokes the same `child_write`
in the parent, retaining the actual writable-open hook. It does not move the
observation point or alter the fixture. The Linux run compiles and reaches the
behavior assertion, failing with `ExecutableFileBusy`, errno 26 (exit 101).
This is the intended RED: a concurrently forked unrelated child inherits the
parent's writer and prevents exec of the completed image. The fixture releases
and reaps that child and successfully executes the same image afterward before
asserting the failed during-blocker attempt, narrowing the failure to writer
inheritance rather than bad image contents.

The pristine Linux GREEN executes the constructor test plus all three materializer
tests: four passed, exit zero. It uses the same platform/toolchain and unchanged
fixture/hook, and its materializer equals the final local production source.
This paired evidence establishes sensitivity to the parent-writer regression and
successful child-writer behavior. GREEN was recorded before the mutation; it is
not evidence of a separate post-restoration full gate. Final gate identity and
cross-platform release qualification remain coordinator responsibilities.
