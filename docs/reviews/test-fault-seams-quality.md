# Test-only fault seams: independent code-quality review

Reviewer: performance_gate. Scope: the newly authorized terminal ABI-response
and guardian external-operation test seams. This review challenges cohesion,
ownership, dependency direction, production cost, and the meaning of the tests;
it does not replace the separate adversarial review or final mechanical gate.

## Guardian review

Reviewed the actual changes in `helpers/guardian/Cargo.toml`, `src/main.rs`,
`src/discovery.rs`, `src/anchor.rs`, `src/successor.rs`, and the three new
`tests/fault_*.rs` files. No P1/P2 finding in this scope.

The three injection calls are private, operation-specific, before resource
creation/transfer, and guarded by `cfg(test)`. The small dedicated seam has no
generic dispatch framework, public API change, or production dependency. Its
thread-local state rejects nested arms, counts consumption, clears on scope
exit, and checks process identity so forked children cannot consume a parent
arm. The guard cannot be moved across threads.

The explicit `harness = false` target runs the real helper modules from a
single-threaded main, preserving the fork precondition. The retained verbose
Cargo command confirms `--cfg test` and no libtest harness. Real PTY, private
session, direct-child ownership, process-group and foreground checks precede
signals. The fixture uses existing Sweep/Cancellation/Workload APIs; it does
not rewrite private state to manufacture success. Each injected operation is
consumed exactly, live workload retention is checked, and actual retry cleanup
or escalation produces a SIGKILL wait status. Channels survive the handoff
error. Scoped cleanup owns and reaps the workload, and the final child sweep
requires ECHILD.

Read retained evidence under `work/helper-fault-recovery`: all-targets has 17
existing unit tests, two startup tests, and all four named real-process
scenarios passing; Clippy passes with warnings denied. Independently rehashed
all eight source entries and the ordinary binary in
`source-and-production.json`: exact matches. Independently ran `nm` on that
binary: no `fault_injection` symbols. No reviewer builds or test reruns were
performed.

Limits: entry faults test caller response to unavailable discovery/anchor or
handoff; they do not prove every socket/fork syscall errno branch. The handoff
case verifies pretransfer failure plus retain/retry cleanup, not a successful
successor transfer. Current real-process evidence is macOS. The collector must
retain and run the new target; final integrated coverage/mechanical/performance
evidence is pending.

## Terminal final review

Static review found a P2 misleading-test oracle in the initial
`fault_scope_is_thread_local_and_clears_on_unwind`: `catch_unwind` also wrapped
the child thread join, so an unexpected isolation assertion panic could be
accepted as the deliberate unwind. The author moved the dedicated isolation
arm and successful join outside `catch_unwind`, then separately tests scope
unwinding. Independent re-read confirms the isolation failure now propagates
and fails the test. The corrected isolation test passes in the retained final
run, so the P2 finding is resolved after re-review and affected execution.

The other terminal seams and tests have no static P1/P2 finding: hooks follow
real FFI calls and are compiled only for tests; malformed values touch only
initialized output; a real long grapheme triggers the capacity retry; tests use
the public factory/ITerminal boundary and exact typed errors, preserve valid
read owners, and require refusal of already-mutated failed owners.

Independently rehashed all five entries in
`work/terminal-native-faults/source-sha256.json`: they match current source.
Read `native-faults-final.log`: all nine tests pass, including the corrected
isolation case. Read `native-existing.log`: all 18 affected native integration
tests pass. Production library Clippy passes and the scoped formatting output
is empty. The retained initial attempt records eight passes and one failure:
the fixture incorrectly expected an immediate `Ok(false)` compression result.
The final test permits real `Ok(true)` progress but requires completion within
128 calls, exact preserved view, and successful subsequent feed. This is a
valid contract correction, not a production bug fix or weakened recovery
assertion. Both attempts remain retained.

Terminal quality conclusion: PASS for this scope, with no unresolved P1/P2.
The final fault test log has an unrelated warning for the concurrently authored
materializer test seam; that separate work still needs its own acceptance.
Final integrated mechanical, coverage, and scoped performance runs remain
required. These synthetic responses establish Rust adapter error/conversion
contracts; they do not claim to trigger arbitrary failures inside Ghostty or
to complete the separate native/platform coverage inventory.

## Materializer final quality review

Reviewed the actual syscall macro and test-only arguments in
`crates/infrastructure/src/process/image_materialize.rs`, the stack fault plan
in `tests/fixtures/process_image_syscall_fault.rs`, and the ten materializer
tests in `tests/fixtures/process_image_materialize.rs`. No P1/P2 finding.

The production macro expands directly to its original libc operation; the
fault plan/arguments disappear outside `cfg(test)`. The post-fork plan is Copy
scalar storage, with no TLS, allocator, lock, callback, or destructor. Its
consumption report is a raw one-byte write to an inherited live socket; errno
is set after that write, so reporting cannot replace the injected errno. Both
socket owners remain alive until the owned child is reaped. Parent report
reading has a timeout and happens only after the materializer returns. The
child never runs parent destructors.

Assertions cover exact consumption, EINTR retries with full bytes and mode,
caller signal-mask equality, pre-fork rejection, opened-inode removal after
write/finalization failures, and clean subsequent calls. The persistent mask
failure case requires both bounded attempts, an error and absent completed
image, observes the actually still-blocked SIGTERM, then uses the real test
RAII guard to restore the original calling-thread mask before retrying. It
does not falsely claim that a failed pthread_sigmask restored the mask. A
synthetic close failure leaves the child's descriptor until its immediate
`_exit`, whose kernel cleanup closes it.

Read `work/coverage-closure/materializer-faults-final02.log`: all ten tests
pass, including the newly named persistent case. The retained first attempt
ran zero selected tests and is not acceptance evidence. Current SHA-256 values
for the reviewed materializer, fault fixture, and tests respectively:
`7749a7a2155710d4d9cfa95753fc3300f425129fc82d5c6db5508a36d5277ddd`,
`46b436cb0dcd922ae2a1a009755002b0c52984bcbcab03db5e985574381241d8`,
`83378b33489ac46a5b9b85c12ae18268e5201f97b4769ea964589ab7e774637e`.
No reviewer builds were performed. Final integrated gates remain pending.

The collector's additional exact `fault_recovery` target and expanded scope
label retain its executable, dependency inventory, test sources and profiles
in the existing checks; this change introduces no denominator exclusion.

### Adversarial finding invalidates materializer acceptance pending fix

The separate adversarial reviewer found P2-1: unrelated concurrent forks can
inherit the report writer and delay `read_to_end` EOF beyond the owned
materializer's reap. The initial quality review above considered only the
owned materializer's descriptor lifetime and missed that concurrent snapshot.
I independently agree: CLOEXEC does not close an unrelated child descriptor
while that child remains before exec. Materializer acceptance is now blocked
pending the proposed parent `shutdown(Write)` after `write_inner` returns,
affected execution, and re-review. Exact marker accounting must remain.

### P2-1 resolved after regression evidence and re-review

Re-read the final `collect` implementation: after `write_inner` returns it now
shuts down the socket's write direction before dropping the writer and draining
the exact bytes. The shutdown affects inherited duplicates, while preserving
already queued markers. No fault-producing materializer remains alive then.
The new regression deliberately keeps an unrelated forked child alive with the
inherited writer; it requires exact `F` receipt and confirms with WNOHANG that
the unrelated child has not exited. Its owned child guard releases/reaps that
specific PID, and the child executes only raw poll/read/_exit with a bounded
poll timeout.

Read the retained `receipt-fork-failing-before.log`: the actual regression
failed in the original reader with `WouldBlock`, not a synthetic failure.
Read `materializer-receipt-fix-green.log`: all 42 infrastructure unit tests
pass after the fix, including that regression, the existing unrelated-fork
image test, all ten materializer tests, and all nine terminal fault tests.
The changed receipt design addresses the actual ownership defect without
weakening assertions. P2-1 is resolved; materializer quality acceptance is
restored for this scope, subject to the final integrated gates.
The corrected receipt fixture SHA-256 is
`4111e4835a2999fa50e937d37cebbfae135c55929a9ed6d094be4406047683b6`;
the other two materializer source hashes remain as recorded above.

## Guardian test-entry layout re-review

The first integrated collector rejected duplicate normalized source spellings
from the new test entry's `tests/../src` imports. The entry now lives at
`helpers/guardian/src/fault_recovery.rs`, uses normal sibling modules, and calls
the existing scenarios in `tests/fault_recovery.rs`. Cargo still declares the
same harness-free target, with the new entry path and `cfg(test)` guard.
Independently compared the full scenario and runner bodies to the frozen first
matrix snapshot: byte-identical except `main` becoming `pub(super) fn run`.
No behavior, assertion, source exclusion, or checker change was needed for
that layout correction.

The retained `helper-fault-layout01` collector subsequently completed its
builds, tests, 23 probes, 54 raw-profile decodes, merge and export. The duplicate
path rejection is gone. It then correctly failed closed under the existing
inventory rule because four TLS macro-generated closure functions identify a
bound installed std source as their primary owner, which that rule currently
requires to be a compiled dependency source. This distinct ownership-model
limitation is unresolved here. Those functions and their regions must remain
in the denominator; this partial collector run is not accepted full coverage
evidence.

## Final fault matrix03 integrity review

Independently inspected `work/coverage-closure/full-test-fault-matrix03` after
all reviewed source/collector fixes. Its 361 source hashes and exact source
population match current source. All 45,710 bound compiler/tool source hashes
match. The three workspace test phases pass; strict workspace and helper
coverage checks fail their unchanged 100% thresholds. Duration: 216.273 s.

Read the final workspace physical auditor and compared it with the previously
reviewed continuous02 auditor: only its output path changed. All 360 audit
input hashes still match; its exact 355-profile population is unchanged. The
audit conserves 42,629 raw function identities into 15,614 merged identities
plus 27,015 zero-only identities over 307,048 counters. It retains 266,831
duplicate records from 145 profiles and combines only exact function/hash
identities with equal counter widths. Independently checked all 60 retained
empty-profile summaries have each numeric summary field exactly zero. All
863,978 workspace export values are nonnegative and below saturation, with
maximum 92,260,214. This review verifies the bound final audit and its method;
the earlier continuous01 review separately re-decoded every raw profile.

Final helper evidence has matching 293 tool, 2,909 std, 389 dependency source,
46 selected dependency source, 59 binary/profile and 32 snapshot bindings, with
exact 54-profile and snapshot populations. All 71 commands and 23 probes pass.
Its physical merge conserves 296 raw identities into 271 merged and 25
zero-only identities over 4,020 counters. The selected source ownership checks
now complete without exclusions.

Coverage remains incomplete: workspace 10,200/10,652 lines, 1,048/1,109
functions, 23,902/27,270 regions; helper 1,875/2,030 lines, 154/155 functions,
4,750/5,655 regions. Branch instrumentation remains unsupported (0/0).
Integrity passes; the mandatory 100% readiness target does not. Mechanical and
scoped performance evidence are being collected separately.
