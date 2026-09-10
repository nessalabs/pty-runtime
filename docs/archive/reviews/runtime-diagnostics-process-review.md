# Runtime diagnostics independent process review

Reviewed the root-authored application diagnostics, runtime/session boundaries,
projection staging/native handoff, process ports, and packaged adapter I/O and
resize instrumentation. This review covers the wired input/output/resize/admission
paths; cancellation dispatch acknowledgements were still pending at review time.

The shared object has eight fixed histograms, each with 1025 atomic buckets and
failure/maximum counters. It retains no session IDs or payloads and introduces no
infrastructure dependency into domain or application. Per-operation timing is
carried only by existing bounded input, resize, or parser-staging entries.

The actual host-read completion timestamp survives reader backpressure retries.
Raw timing finishes after replay publication; projected timing finishes after
successful native feed and continuation journal publication. Input dispatch
finishes after the complete host write, separately from public admission and child
round-trip behavior. Resize finishes after the actual OS call; projected queue time
is preserved. Abandoned or failed admitted operations drop their timer and cannot
inflate successful latency samples. Snapshot consistency is explicitly approximate
until quiescence. Histogram boundary/overflow test (1) and public raw/projected
instrumentation tests (2) passed independently on macOS arm64.

Documentation clarification requested: the default custom-adapter resize port
drops its timer, which increments failures. Describing this as unavailable and
saying a missing boundary stays zero is ambiguous: successful samples stay zero,
but failures increase. State that distinction or add a separate unavailable count.
Read completion still calls `Instant::now` when diagnostics are disabled; this is
a small fixed per-read opt-out cost, not per-sample allocation.

No open behavioral P1/P2 in the reviewed wired paths. A cancellation timing sample
must await the actual signal operation acknowledgement; enqueueing a guardian
control command does not establish dispatch. The subsequent acknowledgement change
requires its own review and tests.

## Cancellation acknowledgement follow-up

Reviewed the subsequent root-authored protocol 17 acknowledgement and helper/host
changes. Actual direct-workload `kill` must return success; reaped/lost/ESRCH paths
cannot manufacture dispatch. Each verified member anchor acknowledges only after
its actual group-zero TERM call succeeds. G records the first TERM root/foreground
targets, requires both group acknowledgements plus direct success, and emits one
owner-only record. Equal root/foreground groups correctly share one acknowledgement.
Late TERM acknowledgements remain valid after KILL has been queued. The host checks
channel, session generation and workload identity before consuming one timer.

Found and reported P2: `request_cancel` checked closed before acquiring its timer
mutex, allowing a paused caller to install a timer after final owner cleanup had
already drained that slot. Root added a second closed check under the same mutex.
Independent reread confirms closure is published before final timer draining, so
creation now either precedes that drain or rejects. Finding resolved by review.

The unavailable-custom-adapter ambiguity is also resolved: a separate unavailable
counter and `Timing::unavailable` distinguish it from failure. The helper codec's
three tests passed after the acknowledgement change. A targeted facade rerun was
initially blocked by concurrent checkpoint inventory edits; final compilation and
runtime tests remain part of the frozen-source gate. The current positive dispatch
test uses one shared root/foreground group; a distinct foreground group case was
recommended before relying on the combined acknowledgement in release benchmarks.

Final acknowledgement verification: added an independent stable, distinct
foreground-group case to `process_foreground_switch`. W remains in its own root
group; a separate foreground member ignores TERM. The case requires exactly one
combined dispatch sample, zero failures/unavailable, complete cleanup, and survival
of an outside-session control process. All three tests in that integration file
passed, including the existing foreground migration scenario. The three current
public runtime diagnostics tests also passed independently after checkpoint
compilation became available. No remaining P1/P2 in this reviewed change.

## Aggregate counters and resource snapshots

Reviewed the later aggregate additions. The active-session flag changes once under
session state ownership at completion, with Drop covering unfinished contexts.
Raw retained bytes use the actual before/after logical replay lengths and context
Drop; capacity is separately reported from the existing replay quota. Delivered
gap counters follow observer advancement and intentionally count each observer.
Read/write counters use actual host operations, including partial writes; admitted
bytes and backpressure attempts have separate definitions. Resource snapshots read
the existing quota objects rather than introducing a mirrored admission ledger.
Quiescent reset clears cumulative counters/histograms while preserving activity and
retention gauges. The four facade diagnostics tests and histogram/reset test passed
independently on macOS.

Requested recovery of poisoned state in `SessionContext::Drop`: the previous
`if let Ok` skips quota and gauge release after poisoning even though Drop uniquely
owns the state. This remains a review item until independently re-read after fix.
The currently reviewed cancellation escalation counter counts host grace-expiry
KILL requests, not actual helper signals; helper-autonomous escalation can precede
host observation, and a queued request can race exit. It must retain that explicit
meaning in reports unless changed to an acknowledged metric with its own missing-
acknowledgement limits.

Poisoned-state Drop follow-up: independently re-read the recovery through
`PoisonError::into_inner`. Quota and gauge release now executes with exclusive
state ownership after poisoning. This accounting finding is resolved; no remaining
P1/P2 in the reviewed aggregate addition. Escalation retains the semantic limit
above until any later protocol change is separately reviewed.

## Separate acknowledged workload escalation counter

Reviewed protocol 18 and the additional `AcknowledgedWorkloadEscalations` counter.
G emits the owner-only record after a successful direct-workload SIGKILL syscall
only when the previous applied control was TERM and emergency cleanup is false.
The host validates channel, generation, workload identity and payload, then counts
it once per lifetime. Endpoint coalescing remains fixed at 19 slots. This measures
observed successful-syscall acknowledgements, with missing transport explicitly
unobserved; it neither replaces host escalation-request counting nor claims signal
receipt or an after-KILL acknowledgement from a member that kills its own group.
No new P1/P2 found. The four current facade diagnostics tests, including one expected
acknowledged escalation, and the new poisoned-context Drop regression passed
independently on macOS.
