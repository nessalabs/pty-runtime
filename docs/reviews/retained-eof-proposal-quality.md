# Retained-endpoint EOF proposal: independent quality review

## Status and reviewed artifact

Current status: the user has explicitly extended the ownership exception to
integrate the EOF correction and complete strict verification. The actual
checkout integration has been independently reviewed below. Historical proposal
sections retain their original pending-permission context; permission is no
longer the blocker for this specific integration. Acceptance gates remain open.

**Scoped quality verdict: no blocking P1/P2 finding in the proposed patch.**
This is a review of an isolated proposal, not authorization to integrate it or
acceptance of the runtime. The production ownership exception is still pending;
the tracked process reader has not been changed. Full gate, platform evidence,
coverage and performance requirements remain separate.

Reviewed `work/coverage-closure/retained-eof-fix.patch`, SHA-256
`1ca5d70028c759fc1db06270160c6abc3e4ee0c5966e070dc22746db74cc7a96`, and the
actual isolated source under
`work/coverage-closure/process-pressure-diagnostic/trace-source`. The artifact
contains seven added production lines in `process/io.rs` and an 83-line public
integration test. The reviewer independently checked the patch hash and source,
read the supplied failure/pass logs, and ran no builds or tests.

## Ownership, control flow and dependency review

The existing reader owns the optional retained child endpoint. In the proposed
`Ok(0)` branch it releases that endpoint only after observing the already
existing `workload_exited` state with Acquire ordering. `take()` transfers the
unique file into `drop`, so ownership is consumed exactly once; no raw descriptor
alias, new shared state or unsafe operation is introduced.

The code then continues the inner read loop. It does not infer final EOF solely
from workload exit or return EOF merely because it closed a descriptor. The next
iteration checks `stop_reader` again and reads the master; subsequent EOF/EIO is
handled by the existing no-retained-endpoint branches. A still-live descendant's
data can therefore continue through the existing read path. Pre-exit `Ok(0)`,
the EIO branch, interruption, output acceptance and error conversion remain
unchanged. An expired drain deadline continues to report truncation through the
existing stop checks; no timeout or deadline policy is relaxed.

This decision belongs in the Unix reader adapter that owns the descriptors and
interprets OS read outcomes. Domain and application types acquire no OS behavior
or dependencies. The change is a cohesive local branch, uses the existing exit
signal and ownership representation, and adds no generalized abstraction or
parallel lifecycle state. The comment explains why post-exit zero-length reads
need special treatment rather than merely narrating the `take()` operation.

The narrow change removes an otherwise repeated read/poll outcome after workload
exit, without changing configured runtime capacity or cancellation semantics.
Its simplicity does not replace behavioral or concurrency evidence.

## Regression test and supplied evidence

The test uses the public process backend and existing event port. A small literal
output fits comfortably within the PTY buffer; the accepting callback waits for
the independently observed process exit, then deliberately delays 350 ms. This
sleep is the injected slow-consumer workload, not a fixed-delay assertion that
assumes some other event has happened. The test asserts exit zero, exact output
bytes and the specific drain outcome. Shared fixture waits bound progress.

The two cases separate the contracts:

- With a two-second drain allowance, the delayed reader must obtain actual EOF.
- With the original 250 ms allowance, deliberately delayed acceptance must still
  report `Truncated`; the patch must not bypass an expired deadline.

`process-pressure-diagnostic/regression-before.log` records the named EOF case
failing with `Truncated`, while the expired-deadline case passes. The supplied
`regression-after-contracts.log` records both cases passing after the patch,
together with all seven existing process contract tests and all four existing
pressure tests. That includes the separate descendant-endpoint truncation
contract. These are meaningful behavioral assertions rather than a private
branch counter or a test-only constructor.

The deliberate delay plus a two-second upper allowance still depends on host
scheduling. Passing locally establishes the recorded executions, not arbitrary
host-load reliability. The supplied evidence is macOS-scoped; Linux behavior
needs its own executed checks before a cross-platform acceptance claim.

## Remaining acceptance conditions

The proposal has not repaired the tracked CI branch yet. Preserve the failing
evidence and obtain the required ownership permission before integration. If
approved, rerun affected contracts and the mandatory independent/mechanical
gates against the actual integrated source. Existing coverage and runtime
performance failures remain open; this review neither waives them nor treats
the isolated passing tests as a new qualified performance cohort.

## Frozen proposal re-review: delayed descendant output

Re-reviewed the final patch SHA-256
`55aefd7e61f27bacdbfba1e1d0ff540f1f0c30003a22fc97ff76f4d458103c63`.
Its production correction is unchanged. The new test file now has 135 lines,
including a third independently authored preservation test. Verified the patch
hash and that the test in the isolated snapshot matches its patch contents.

The new case uses the existing public backend and a real shell descendant. A
release file provides an explicit ordering boundary: the test observes workload
exit and absence of drain before allowing the descendant to write `tail`. Exact
`initialtail` bytes, workload exit 23 and EOF are then required. This is stronger
than relying on a sleep to guess when the workload exited and specifically checks
that releasing the reader-owned endpoint does not discard a live descendant's
later output. It is preservation coverage, not an additional reproduction of the
modified macOS zero-read branch.

The fixture's directory has scope-bound cleanup. The runtime/backend is declared
after the directory guard, so backend teardown precedes removal of the release
directory during unwinding; normal completion also calls shutdown explicitly.
The child's polling loop has an iteration bound, preventing an indefinitely
waiting fixture after an assertion failure. Literal argument passing keeps the
temporary path out of shell-source interpolation. Polling delay and the explicit
two-second drain allowance still impose scheduling assumptions; no broader
latency guarantee is inferred.

No new abstraction, production dependency or domain policy is introduced. This
test remains cohesive with the two drain/EOF cases and exercises behavior through
the public process boundary. `adversarial-delayed-descendant.log` records all
three tests passing in 0.72 seconds; the earlier before/after logs retain their
original two-test scope. **Scoped quality verdict remains PASS with no blocking
P1/P2 finding in this delta.** Full-gate, cross-platform, coverage/performance and
ownership-permission conditions remain outstanding. This re-review ran no builds
or tests and did not modify production code.

## Authorized actual-checkout integration review

Reviewed the actual diff at base commit
`a3e0fee3c5974e22486673d94cade302ad9b39ee`, source digest
`e986497052ef14b8e5f1aeb65920a61ec7d9e2ea5942fb1b2e005a5b557210e0`.
The seven-line reader change and untracked 135-line `process_delayed_eof.rs`
were read in the actual checkout and independently compared byte-for-byte with
the previously reviewed frozen isolated snapshot; both match. The user has
authorized this production integration and its strict verification.

Scoped Clean Code/SOLID and DDD/dependency verdict: **PASS, no unresolved P1/P2
finding in the actual integration**. The descriptor is still uniquely owned by
the reader; the exit state is observed rather than invented; release occurs at
most once; a subsequent read confirms EOF; and stop/deadline handling precedes
that read. OS-specific behavior remains in infrastructure. Public descendant
and expired-deadline tests remain exactly as reviewed, with their bounded
coordination and existing scheduling limitations.

The actual full gate at `work/coverage-closure/eof-integrated-gate` is running;
this review does not assume its result. Isolated passes are retained evidence,
not substitutes for actual-checkout, hosted, coverage or performance validation.
No builds, tests or production edits were performed by this reviewer.

## Actual mechanical and coverage evidence

The completed actual-checkout mechanical run at
`work/coverage-closure/eof-integrated-gate` reports **PASS**, exit zero,
124.216 seconds, and unchanged sources on macOS 26.6 arm64 with Rust 1.98.1.
The reviewer read its metadata and terminal log result; the separate adversarial
review checked the recorded source inventory. This now supplies actual-checkout
mechanical evidence rather than relying on the isolated snapshot.

Strict coverage at `work/coverage-closure/eof-integrated-coverage` reports
**FAIL**, 118.958 seconds, with unchanged sources. Every compilation/test phase
exited zero; only the two strict report phases exited one. Independently inspected
export totals are:

| Scope | Lines covered/total | Functions covered/total | Regions covered/total |
| --- | ---: | ---: | ---: |
| Rust workspace feature matrix | 9,959 / 10,535 | 1,029 / 1,104 | 14,611 / 15,737 |
| Separate helper test run | 476 / 1,478 | 62 / 109 | 747 / 2,274 |

These are not 100% results. The command does not measure native C, upstream native
dependencies, other platform configurations, Python/build tooling or Rust branch/
MC/DC coverage; zero branch counters mean unmeasured, not complete coverage.
The actual MSRV matrix is still pending at this record update. No performance
acceptance is inferred: the earlier candidate failed and the new fixed EOF cohort
has not yet been collected. Scoped code-quality PASS and mechanical PASS therefore
do not make the overall integration ready.
