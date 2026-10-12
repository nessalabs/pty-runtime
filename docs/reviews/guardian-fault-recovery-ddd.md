# Guardian fault recovery: independent DDD and ownership review

Scoped PASS; no new P1/P2 dependency-direction or ownership finding in the
reviewed helper delta. This follows the user's new authorization for minimal
test-only external-operation failures. Reviewed the actual Cargo/module changes,
three fault entry checks, scope implementation, single-threaded fixture and four
scenarios against the cleanup/cancellation/successor/workload contracts. No build
or scenario was run by this reviewer. All eight source hashes and the retained
ordinary helper binary hash in `work/helper-fault-recovery/source-and-production.json`
independently match current files.

The three seams sit at operations that own acquisition: discovery creation,
anchor creation and successor handoff. An injected error returns before any
socket, forked child, or endpoint transfer for that operation. It does not alter
a caller's private state, forge successful process identities or skip cleanup.
The unchanged calling component decides whether to retain uncertainty, retry,
or preserve the grace policy. This keeps acquisition failure separate from the
cleanup policy under test. It also means the tests do not establish individual
socket/fork syscall failure arms inside those operations; the author correctly
states that limitation.

All production references and the fault module are under cfg(test), with no
runtime option, environment variable, feature switch or public domain port.
The dedicated integration executable has a compile_error outside cfg(test).
The saved verbose Cargo command confirms `--cfg test` with no `--test` harness;
its explicit main is single-threaded. The test target imports helper modules
rather than creating a second production implementation. Cargo's ordinary helper
build has no fault module/checks; the recorded no-symbol inspection is supporting
evidence, not the sole argument for absence.

The injection owner is a bounded thread-local Cell with a PID-tagged arm.
The non-Send/non-Sync guard cannot migrate across threads, rejects nested scopes
and clears the arm on drop. A forked child inherits bytes but has a different PID,
so it cannot consume the parent's fault. This avoids a global cross-test switch
and an inherited mutex. Each scenario checks the exact remaining count before
removing the arm. Fault consumption is therefore established rather than inferred
from a generic later failure.

The fixture creates a private controlling-terminal session and verifies its SID
and process group differ from the inherited runner's group. Every workload is an
actual unreaped child whose SID/PGID and foreground identity are checked after
readiness. Production Workload signalling retains the unreaped-child PID
safeguard. Sweeps/anchors use their existing verified session/group behavior;
the fixture does not authorize signalling an arbitrary discovered numeric PID.
The single-threaded executable owns all children, which is the explicit
precondition for its final waitpid(-1) cleanup; that helper must not be copied
into a shared libtest worker or concurrent child-owning runner unchanged.

Discovery/anchor scenarios verify retained ownership and a live child before
retry, then require real SIGKILL wait status and completed sweep. Failed handoff
proves both links remain usable and the caller-facing retry operation preserves
cleanup ownership. It manually invokes that retry API: it is not a whole
Guardian event-loop regression or evidence of successful successor transfer.
The unavailable-anchor case checks no false TERM group acknowledgement, exact
grace boundary through explicit Instant inputs, one escalation receipt and real
child SIGKILL/reap. It establishes cancellation component behavior; existing
whole-helper probes remain the separate event-loop oracle. None of these
assertions is satisfied by the LiveWorkload fallback destructor alone.

Saved all-target evidence records 17 original units, two startup tests and four
custom-main scenarios passing; Clippy output and the normal build pass. Existing
coverage collector target membership now explicitly includes `fault_recovery`,
so the added declared target is not silently dropped. The compiler/test source
population, new fixture code and fault-only branches remain in the denominator.
The fresh canonical run must establish actual coverage; no 100% or complete
quality-gate claim follows from these focused passes.

Failure-cleanup limits remain bounded: the test guards try actual-child cleanup
and impose deadlines, but a fatal assertion during unwinding can terminate the
test process. This is not a new production recovery guarantee. Production syscall
retry faults and native-contract injection authored elsewhere require their own
actual-code review; this scoped result does not pre-approve them.
