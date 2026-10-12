# Guardian public gap probes: independent adversarial review

Decision: static review has no blocking P1/P2 finding. This is not execution or
final acceptance; the actual 23-case run and affected quality checks are pending.
Reviewed the three added scenarios in `scripts/guardian/probe.py`, their teardown,
unchanged Session mechanics, and actual guardian/cancellation/workload/sentinel/
links/protocol contracts. No helper, probe, build, test, or coverage collection was
executed. Static AST comparison found no changes to original non-main functions
or classes. Main retains the earlier 20 calls and appends three cases.

The TERM probe waits for the ignoring shell's readiness, verifies its actual SID,
root PGID and foreground group, then writes only Terminate to the guardian owner
channel. Both owner channels stay open and are pumped. The shell has no external
command timeout or descendants that could create another explanation for exit.
Success requires the exact SignalApplied target/signal/flags, one guardian
Escalated receipt, one guardian WorkloadExit, the specified order, and actual
SIGKILL wait status with the workload identity on every observed exit receipt.
StartFailed/Fault are rejected; the session must have no live members before any
teardown call. The existing outside-session control is checked after all cases.
This distinguishes helper backstop from owner EOF, owner Kill or cleanup.

The order assertion is consistent with the source contract: pending endpoint
frames flush by numeric kind, so SignalApplied precedes Escalated if both become
pending together. Guardian sends escalation during cancellation.tick and flushes
before the next loop's workload reap. The tiny actively drained control stream
does not intentionally introduce backpressure. The one-second grace still
assumes ordinary scheduler progress for TERM/anchor acknowledgement; severe
scheduling delay can cause a timeout/assertion failure, not a false success.
This case shares root and foreground groups; it does not establish independent
signalling of a distinct foreground group.

The Execute cases use fresh isolated paths, including an existing mode-0600
script whose accidental execution produces output. They require Hello from both
endpoints before Execute, exact ENOENT/EACCES StartFailed values on both channels,
and no Started, Admitted, WorkloadExit or Fault. Both owner EOFs and actual helper
reap must occur before checking absent runtime identity/output and no live SID
members. Thus a rejected request or test cleanup cannot impersonate successful
launch followed by teardown. Socket sends swallowed by the old Session.send
helper do not yield false success: the missing errno receipts make the new
assertions fail.

`close_gap_session` runs only after the success oracle or on an exception. It
uses existing Abort/owner EOF cleanup, closes both local sockets and PTY even if
normal close fails, then bounds fallback wait after killing only its actual
Popen child. No numeric discovered process/group is force-killed by the test.
A pathological broken guardian can still outlive failed teardown; this static
review does not certify arbitrary failure cleanup under simultaneously broken
recovery owners. Normal successful scenarios must demonstrate zero live members
before teardown, so that limitation cannot produce a passing leak assertion.

# Collector ownership integration review

Also inspected the coordinating author's collector wiring in
`scripts/guardian/coverage.py`. Source membership comes from dep-info for the
actual helper and all selected unit/startup executables. The explicit test target
set fails on missing or unexpected names. The Cargo manifest marks the standalone
workload fixture binary test=false; it remains a fixture, not a silently removed
test target. External helper test modules and compiler-source macro aliases stay
in the audited LLVM export and totals. The collector does not substitute a
production-only subtotal, filter out alias records or lower expected coverage.
Darwin's uncompiled Linux discovery source remains explicitly unmeasured.

Each captured raw profile is read before merge; sparse merge conservation permits
only all-zero records to be absent. Raw/merged physical saturation and exported
count saturation reject evidence. Final checks bind selected binaries/profiles,
copied source contents and membership, and installed-source contents/membership.
`--require-complete` requires nonempty, exactly covered line/function/region
totals; a successful partial collection without that option is not 100% acceptance.
No branch or other-platform completeness is implied.

Independence boundary: `coverage_checks.py` derives from the inventory checker
previously authored by this reviewer. This section reviews the new collector's
ownership/wiring only and does not independently approve that validator. Its
separate quality/adversarial review and current test evidence remain required.
No fresh measured results were available to this review; runtime validation,
linked-tool/profiler calibration and the complete quality gate remain open.

Reviewed probe SHA-256:
`6aa493dd0f32c0116059ee7aafbc85c6637dbe03d0dd482fbba5d44020218435`.
Reviewed collector SHA-256:
`57707fa9a718c73e65997fd3e43f109c0fe6ac8d107b508bc60a751d3d24a9d6`.

## Fresh collection 03: actual effects and final wiring re-review

**Bounded execution/wiring acceptance; no new P1/P2 found. The 100% gate fails.**
Inspected retained `work/coverage-closure/helper-continuous-current-evidence03`
without executing any helper, build, test or collection. All 68 recorded child
commands exit zero: 17 helper units, both startup tests and all 23 actual protocol
scenarios pass. The final collector remains unsuccessful at its required complete
coverage check: 1,581/1,739 lines, 124/127 functions and 3,952/4,716 regions. This
must not be relabeled a passed quality gate or complete helper coverage.

The executed snapshot probe is byte-identical to the previously reviewed current
probe (SHA-256 `6aa493dd0f32c0116059ee7aafbc85c6637dbe03d0dd482fbba5d44020218435`).
Its final three receipts report TERM-only guardian backstop with actual exit -9
and both signal acknowledgements, ENOENT 2, and EACCES 13. These pass records can
only be returned after the reviewed exact protocol-frame/wait-status assertions,
closed endpoints, helper reap and empty-live-SID assertions. No failure stderr is
recorded. The original outside-session control is checked after these cases and
before exit zero. No production code or earlier scenario assertions were changed.

Raw protocol frame arrays and process snapshots are not serialized on successful
runs: the evidence contains the completed source assertions and success receipts,
not an independent replayable packet/SID trace. Therefore this review accepts the
observed test execution and its bound source oracle; it does not claim separately
reconstructed raw frames or an independently sampled historical process table.
The earlier distinct-foreground/scheduler and pathological failure-cleanup limits
still apply.

The final collector SHA-256 is
`1973d9ac691bd3dea6c328a6bd07d5b009351fbd98ddcbddf1a33b62944ecb24`, identical
between current file and executed snapshot. Cargo's retained JSON includes a
fixture harness with profile.test=true but target.test=false. The final filter
requires both and retains exactly the declared guardian unit and startup test
objects plus the actual helper. This enforces the manifest's explicit disabled
fixture test policy, without removing a declared test. The export command lists
all three objects. The measured population has 20 physical sources, including
links/public_boundaries/startup tests and three installed compiler-source aliases.
No test-module or alias denominator has been dropped.

Independently matched all 28 copied source hashes, five pre-run object hashes and
57 final binary/profile hashes, and checked exact copied-source and 53-profile
membership. The physical audit records 180 raw identities, 170 merged identities
with ten all-zero sparse omissions, and 2,555 conserved counters. The export audit
checks 12,586 values without saturation. These are the retained validator results;
this reviewer remains a prior author of its base checker and does not supply its
independent acceptance. The final llvm-profdata commands explicitly request
all-functions/counts, matching the pinned tool's actual output format. Historical
failed attempts remain failure evidence and are not used as a successful run.
Also independently matched all 293 measurement-tool hashes and 2,909 installed
Rust-source hashes in the run's pre-run manifests; no current mismatches.
