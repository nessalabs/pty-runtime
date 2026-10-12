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

## Combined stack coverage audit at 55174b8

Independently inspected `work/coverage-closure/pr25-fixed-stack-coverage02`
after both fixes and the dependent oracle correction propagated. The unchanged
strict runner completed in 310.439 seconds. All three workspace functional
feature phases pass; the workspace and helper strict completeness reports fail.
Workspace: 10,195/10,646 lines, 1,049/1,109 functions, 23,892/27,264 regions.
Helper: 1,871/2,030 lines, 154/155 functions, 4,732/5,655 regions. Branch and
MC/DC coverage remain unsupported; native and other-platform coverage remain
outside this measured scope. There is no 100% pass and no merge acceptance.

All 361 recorded source hashes and 45,710 compiler/tool-source hashes match
current files. The workspace physical-counter auditor differs from the
previously reviewed auditor only in its output-directory selection. All 360
bound audit input hashes match, as does the exact 355-profile population.
Independently parsed all 60 retained empty-profile summaries: their function,
block, and total counts are zero. The bound audit conserves 307,117 counters
across 42,632 raw identities into 15,616 merged identities plus 27,016
zero-only omitted identities; it accumulates duplicate physical records.
The exported 864,160 count values have maximum 100,643,809 and no saturation.
This re-review checks the bound auditor and its input integrity; it does not
claim a second independent full LLVM decode of every raw profile.

Helper inventory hashes match for 32 source inputs, 293 tool inputs, 2,909
installed Rust source files, 389 dependency files, and all 59 binary/profile
inputs. Its exact raw population is 54 profiles, and all 71 command records
pass. Its bound counter conservation is 296 raw identities into 272 merged
plus 24 zero-only omitted, with 4,020 counters and maximum raw sum 328,975.
The export checks 16,809 counts, maximum 339,583, without saturation; helper
readiness totals match the retained export. Integrity review passes within
this measurement scope; the strict completeness result remains FAIL.

## Final-stack mechanical and scoped performance re-review

`work/pr25-blockers/final-stack-gate01` passes in 195.998 seconds at 55174b8;
all 361 recorded source hashes still match. Independently reviewed prospective
`work/coverage-closure/pr25-final-stack-bound-performance01`: all 1,607 bound
artifact/native/tool hashes match before, after and current files, including
the exact 355-file release population. The collector differs from the
previously reviewed collector only in its output directory. Cargo JSON selects
one executable for each workload, and the nested Running paths select those
exact executables without compilation/checking/dirty artifacts during capture.
Neither executable contains LLVM profile sections. Runtime, domain and bundled
helper symbol inspections and the selected dependency files show no reviewed
test fault-injection code entering these release measurements.

Runtime trial at 1/16 sessions completes in 49.284/705.021 ms, with startup
258.247/209.395 ms and cleanup 1.648/1.391 ms. Validated raw and parsed counts
match at 4,194,326/67,109,222 bytes; both shutdown samples retain five file
descriptors and zero descendants. Post-processing process RSS is
11,517,952/147,931,136 bytes. These are process point samples, not total or peak
memory. Domain evidence retains all 25 correctly ordered positive trials;
median ordinary/oversized/query/saturated/one-byte times are respectively
0.861/12.270/6.316/13.414/10.144 ms. All original raw trials remain retained.

Scoped performance review passes. This establishes the stated workloads and
artifact identity, not a relative regression threshold, speedup, full release
qualification, repetitions or soak. The strict coverage failure above remains
a merge blocker. No reviewer builds or benchmark reruns were performed.
