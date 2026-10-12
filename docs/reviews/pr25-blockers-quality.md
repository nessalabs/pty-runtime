# PR 25 blocker fixes: independent quality review

Reviewer: performance_gate. Base: `3490f98` on `codex/osc7501-domain`.
Scope: duplicate-app parsing and benchmark interruption cleanup, followed by
the required frozen-source gates. No reviewer builds or source edits.

## Parser fix

Reviewed the actual `report.rs` diff and independent public scan/commit/records
regressions. The linked OSC 7501 revision 0.4 grammar skips malformed pairs
before duplicate-key replacement. Values containing `!`, internal whitespace,
or non-UTF-8 bytes are malformed and must preserve an earlier valid app. Comma
and empty values are syntactically valid but fail app-specific semantics, so
they replace the earlier app with absence. A later valid app replaces it.

The fix removes the early app assignment and UTF-8-error app clearing, then
handles app in the existing match after the general ASCII value grammar.
Hard byte limits remain before malformed-value skipping; invalid ids still
reject the whole report. Ownership, allocation, dependency direction and store
semantics remain unchanged. No P1/P2 finding in this parser change.

The corrected regression replaces an earlier test that conflated malformed
pair syntax with invalid app semantics. It preserves both contracts and adds
BEL/ST framing, every two-chunk split, later state/message processing, and the
32/33-byte app limit. This is a specification correction, not assertion
weakening. Retained `work/pr25-blockers/domain-acceptance-red.log` has the two
expected retention failures among 19 tests; `domain-acceptance-green.log` has
all 19 passing after the production fix.

Reviewed parser SHA-256:
`79dbcf2811d6e0386a7cc5bc2dec1a2bd69159d7f50ab38147613e7a82fd99fe`.
Reviewed acceptance-test SHA-256:
`99ace72430ee272720b11fa1606045f79712edf669b675509b6a78f4401bc724`.

## Benchmark cleanup review

The independent real-SIGINT regression uses a live launcher and descendant in
the benchmark's own process group. It verifies propagated KeyboardInterrupt
and checks both processes before releasing the fixture cleanup channel, so
the fixture's cleanup cannot manufacture a passing lifecycle assertion. Its
fallback kills the fixture's own current group after EOF, avoiding cached
descendant/group-number cleanup signals.

The retained `work/pr25-blockers/interrupt-red.log` shows the new regression
failing because launcher 34038 remained live (`Ss`); existing success and
timeout tests passed. Production cleanup and `interrupt-green.log` were then reviewed: all three
real-process success/timeout/interrupt tests pass. Cleanup is a cohesive helper
with one bounded deadline, keeps ownership checks before group signals, and
retries interrupted cleanup operations before re-raising cancellation. Final mechanical, coverage, performance, specialist reviews,
and stack/merge prerequisites remain open. Prior strict coverage failure is
not waived by these scoped fixes.

### P2: supported Python cancellation is replaced by AttributeError

On the reviewed production diff, `run_benchmark` calls
`interruption.add_note(...)` if its cleanup helper returns failures. This API
requires Python 3.11; README declares Python 3.9+ and the local gate interpreter
is Python 3.9.6. A cleanup OSError, timeout, or repeated-interruption deadline
therefore replaces the intended KeyboardInterrupt with AttributeError. An
independent bounded orchestration reproduction patched Popen to interrupt and
the cleanup helper to return a failure; Python 3.9.6 reported
`AttributeError: 'KeyboardInterrupt' object has no attribute 'add_note'`, with
KeyboardInterrupt only as exception context. This is an orchestration check,
not real-process cleanup evidence. Preserve the original cancellation and
cleanup diagnostics on supported interpreters, add targeted evidence, and
re-review. This P2 was open pending the re-review below.

### Re-review: resolved

Read the final helper and all seven durable regressions, plus retained
`work/pr25-blockers/interrupt-edge-green.log` (7 passing tests, 1.369s on
Python 3.9). Cancellation now stops an unreaped group, closes both streams,
and waits using the remaining shared deadline. It does not re-enter
communicate, so cancellation during timeout cleanup after stream closure does
not produce the previously identified closed-pipe ValueError. Cleanup failures
are attached as a tuple to the original exception before bare re-raise; this
works on Python 3.9 and preserves the exception object and type. The add_note
P2 is resolved. No new P1/P2 quality finding in the final scoped change.

The four additional tests verify orchestration only: closed-pipe reentry,
repeated interruption with decreasing wait budgets, no group signal after
the leader is reaped, and preservation of original cancellation plus multiple
cleanup failures. The separate real-process tests cover actual success, timeout
and single-SIGINT launcher/descendant cleanup. This division is appropriate: a
mock ordering check is not being presented as evidence of OS process cleanup.

The ten-second deadline limits cleanup waits and repeated operation retries;
it is not an uninterruptible OS guarantee against arbitrary signal storms.
Recorded cleanup failures do not imply successful reaping. A launcher already
reaped with unrelated surviving descendants is outside the stated Cargo-owned
lifetime assumption; a stale cached PID is deliberately not signalled. The
change introduces no new Rust domain dependency or hot-loop work and keeps
cleanup responsibility in the benchmark runner. Mechanical checks, frozen-source
performance, strict coverage, and stack/merge prerequisites remain separate;
prior strict coverage failure is not waived by this review.

Final reviewed hashes:

- `scripts/program_status_performance.py`: `7fff923f914678b39c303755714c8f236f1f7c2ddd5c2e965b430403c77892a8`

- `scripts/tests/test_program_status_performance.py`: `7291b8f2999cdd2524228aed164ce7c72df9f0f8954d6863b3f043283954cfcc`

## PR 26 dependent malformed-app oracle correction

Independently reviewed the eight-line diff in
`crates/domain/tests/program_status_coverage.rs` after the parser fix propagated
from PR 25. The old UTF-8 duplicate-app oracle expected both raw and resolved
app to be absent; that contradicts the reviewed protocol rule that malformed
pairs are skipped. The correction expects both to preserve `earlier` and adds
a subsequent valid base64 message with decoded value `ok`, proving processing
continues after the malformed pair. Existing record-count, state, kind and
invalid-id root-protection assertions remain intact. Production is unchanged.
This is a corrected oracle, not reduced test coverage or weakened assertions.

Reviewed retained `work/pr25-blockers/pr26-stale-oracle-red.log`: the original
oracle failed with actual `Some("earlier")` versus expected `None` (4/5 passed).
The green log retains all five coverage behavior tests plus all 19 parser
acceptance tests passing. Those acceptance tests independently distinguish
malformed syntax from syntactically valid but semantically invalid app values.
Scoped quality review passes with no P1/P2 finding. This does not waive the
separate strict coverage requirement or constitute a whole-stack gate pass.

Reviewed test SHA-256: `52b45ccbf230b743a20467227a84c95364a6022cac3c0d949931a97844370a88`.
