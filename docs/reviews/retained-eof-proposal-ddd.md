# Retained-endpoint EOF proposal: independent DDD review

**Scoped DDD/public-contract PASS for the isolated proposal; NOT INTEGRATED.** Reviewed `work/coverage-closure/retained-eof-fix.patch`, SHA-256 `1ca5d70028c759fc1db06270160c6abc3e4ee0c5966e070dc22746db74cc7a96`, and its applied source in `work/coverage-closure/process-pressure-diagnostic/trace-source`. This reviewer authored neither the proposed production correction nor its regression tests. No builds were launched.

## Ownership and dependency reasoning

The correction stays inside the infrastructure PTY reader, which already owns the master read loop and retained child descriptor. It adds seven lines only to the successful zero-byte read branch. When the retained endpoint exists and the existing workload-exited flag is observed, the reader releases its own endpoint and continues reading. It does not infer workload exit from an empty read, change domain state or expose native descriptors through application ports. The flag still comes from `Session::note_workload_exit`, whose release publication denotes a reaped workload; the reader's acquire load follows that existing boundary.

Descriptor lifetime belongs here because it implements the adapter's output-drain behavior. Domain continues to own `DrainOutcome`, and application continues to expose the events contract. No dependency is added or reversed, no policy moves into tests, and no redundant coordinator or state machine is introduced. Existing pre-exit handling remains unchanged. The EIO branch remains unchanged. The reader's stop checks, production drain deadline and per-process defaults are unchanged.

The added continuation is significant: dropping the retained endpoint does not itself report EOF. The next real read confirms EOF, reads further bytes, waits for more descendant output or follows the existing typed failure path. A requested reader stop is checked before that next read and remains Truncated. This preserves the distinction between actual EOF and deadline cutoff rather than rewriting one outcome into the other. There is at most one new endpoint release per reader because `take()` removes its ownership.

## Regression contract and evidence

The 83-line integration fixture launches a real PTY workload and injects delay through the existing `IProcessEvents::output` boundary. It first observes actual exit via the independent callback and then delays acceptance for 350 ms. The two cases intentionally distinguish a 2-second drain budget from an expired 250 ms budget; both retain exact `literal` bytes and exit-zero assertions. It does not inject private reader state, change default limits, skip EOF checks, retry until passing or pretend that complete byte count proves EOF.

`process-pressure-diagnostic/regression-before.log` records the long-budget case failing with Truncated while the expired-budget case passes. `regression-after-contracts.log` records both regression cases passing after the proposal, alongside all seven process contracts and four pressure tests, including the separate descendant-endpoint cutoff contract. This before/after evidence is specific to the isolated source and local platform. The deliberately delayed callback tests a finite collaborator stall; it does not establish a universal callback latency guarantee or eliminate all scheduling variation.

## Limitations and integration status

No P1/P2 DDD or public-contract defect was found in the proposed change. Separate adversarial review must assess native EOF semantics, races and cross-platform behavior. The evidence does not establish Linux execution, hosted-CI closure, full mechanical acceptance, final-source coverage or integrated performance. Those remain required for the actual integrated candidate.

The working production tree was not changed by this review; its process reader and regression path have no pending changes. The proposal remains isolated because the user's ownership instruction prohibits editing the separately owned implementation without the required ownership exception. This scoped pass supplies technical review evidence only and grants no integration authorization. The earlier CI-closure P2 remains open until an authorized integration and its required verification complete.

## Final patch delta re-review

An independent read-only re-review covered final patch
`55aefd7e61f27bacdbfba1e1d0ff540f1f0c30003a22fc97ff76f4d458103c63`, including the
third public integration test authored by the adversarial reviewer. The seven-line
production correction is unchanged. The new real descendant waits for a release
file created only after the test observes workload exit, then emits its final
bytes; exact combined bytes and EOF are required. This tests the existing
application/process contract without manipulating domain or reader internals.
Filesystem coordination and shell process details remain in the infrastructure
test, with no new production dependency edge or cross-layer information owner.

The supplied `adversarial-delayed-descendant.log` records all three cases passing.
Scoped DDD verdict remains PASS for this delta, with no P1/P2 finding. It is
preservation coverage, not proof that the new test forces the specific `Ok(0)`
branch, nor a Linux/hosted acceptance result. The proposal remains isolated and
pending the ownership exception and mandatory full verification. No builds or
tests were run during this re-review.


## Authorized integration re-review (2026-10-11)

**Scoped DDD/dependency/public-contract PASS for the actual integrated working
source; overall acceptance remains open.** The user explicitly extended the
ownership exception to apply the retained-endpoint EOF fix and complete
integration verification. The historical proposal-only statements above describe
the earlier review state and no longer describe the current integration status.

Reviewed the actual `git diff` against base
`a3e0fee3c5974e22486673d94cade302ad9b39ee` and the actual new test file in
`/Users/nessa/Documents/NessaLabs/pty-runtime-program-status`, rather than assuming
the proposal had been applied correctly. Production
`crates/infrastructure/src/process/io.rs` SHA-256 is
`eee5294c8b42105278a5c240c0a72cdfb97c0eeacce7be4d5d1edbf10140c709`;
`crates/infrastructure/tests/process_delayed_eof.rs` SHA-256 is
`f6a1142f8babd373b0fb580116197612756102e89ed2ccf7b32cc0be40a73d99`.
The integrated seven-line production delta and three tests match the final
proposal's reviewed behavior.

Re-traced the information owners: `OwnedProcess::record_exit` translates the
supervised exit into `ExitStatus` and calls `Session::note_workload_exit`, which
publishes the existing flag with release ordering. The reader uses acquire
ordering, disposes only of its own retained endpoint after an actual zero-byte
read and observed workload exit, and confirms the resulting read outcome on the
next iteration. It neither fabricates an exit from EOF nor reports EOF from
workload exit alone. Descriptor ownership remains wholly in infrastructure;
`IProcessEvents` and domain `DrainOutcome` retain their existing separation of
exit, EOF, truncation and typed failure. No new public API, native representation,
dependency edge, allocation or cross-layer policy is introduced.

The unchanged stop check precedes the confirming read. A pending stop therefore
still yields Truncated. Output admission retains its existing whole-chunk
contract and the tests use that public boundary without private-state injection.
The descendant preservation test asserts that observing workload exit does not
itself permit a drain report and requires the descendant's released tail before
EOF. These are coherent adapter contract tests; no test fixture is promoted to a
production capability claim.

No P1/P2 DDD, dependency-direction or public-contract finding was found. No
production changes, builds or tests were performed by this reviewer. Read the
retained before/after and descendant evidence in
`work/coverage-closure/process-pressure-diagnostic/FIX-EVIDENCE.md`; its isolated
results remain useful causal evidence, but do not replace the actual integrated
gate. At review time `work/coverage-closure/eof-integrated-gate/command.log` was
still running. This review does not claim that gate passed, that coverage reached
100%, or that scoped/full release performance, Linux execution or hosted CI
closure has passed. Coverage and performance remain open acceptance limitations,
with their actual integrated outcomes to be recorded by the owner.
