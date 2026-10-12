# Journal, public contract tests and canonical coverage: independent DDD review

Decision: scoped source/ownership pass; no new P1/P2 finding. This review is
separate from coverage readiness, performance, and the complete quality gate.
Reviewed the actual tracked changes in journal.rs, projected_runtime.rs,
projection_correctness.rs, its clock_failure module, and scripts/coverage.py,
with the production runtime owner/session/attachment/admission contracts. The
coordinator permitted this documentation write during the running source freeze;
no Rust/Python source, build, test or measured process was changed or executed.

## Production ownership and public test boundaries

The seven journal substitutions delegate directly to PoisonError::into_inner.
They introduce no domain policy, adapter dependency, state representation or
synchronization abstraction. Lock acquisition, returned guard, recovery policy,
closure return ownership and guard lifetime are unchanged. In particular, waker
extraction and the positions of mutex release and arbitrary waker drop/wake
callbacks remain unchanged. This is redundant-closure cleanup, not a new fault
recovery behavior; the clock test is not evidence for journal poisoning.

The public service replacement test follows the consuming with_projection
contract. A configured Runtime owns its original projection services; a second
configuration is rejected, and the consumed owner is dropped, closing the old
projection. The retained session observes Closed and the old terminal owner
releases. The rejected replacement is never installed; its retained caller
clone remains responsible for stopping its worker pools. The test checks both
old-owner release and lack of replacement terminal construction/feeding through
public status and injected adapter observations. It neither reaches into private
runtime state nor moves test-specific configuration into production.

The raw resize assertion likewise tests the existing public split between raw
and projected operations. A projected session rejects raw resize as Unsupported
before a process resize is admitted. Control generation, processed cursor,
failure and terminal view remain unchanged, followed by the existing successful
projected-resize assertions. This retains the coordinator as owner of joint
process/model resize sequencing.

CallerClock and EventBackend are test implementations of existing IClock and
IProcessBackend ports. The backend obtains the public event sink at spawn. The
clock disarms only on the callback's calling thread, releases its own mutex and
then panics; worker clock reads cannot consume the injected failure. The explicit
disarmed assertion identifies that collaborator as the source of the caught
panic. Production output admission holds context/admission state before calling
the clock; the test checks the existing typed failure policy, no publication,
lease rollback and owner cleanup rather than a new promise to recover arbitrary
panics transparently.

The added pending-waiter case admits completion and output observers, confirms
both are Pending, triggers the same public callback failure, then polls each
again and requires Ready(Internal). Attachment position stays unchanged and
synchronous reading reports Internal. Dropping waits before runtime shutdown
preserves observer ownership. Waker::noop is appropriate for this explicitly
polled failure contract: the test establishes neither autonomous notification nor
executor progress after a panic. No claim of such progress should be inferred.

## Measurement composition

The canonical script remains an outer composition/acceptance owner. Darwin
workspace flags enable atomic, page-aligned continuous counters using a fresh
run-owned build/profile root. Inherited profile naming is removed before setting
the intended %p%c name. The workspace continues to use the existing
uninstrumented-helper composition seam so the independent Empty environment
policy assertion remains intact. The separate helper collector exercises actual
helper units, startup and protocol behavior using continuous counters for
fork/_exit/group death; no production shutdown hooks or workload environment
exceptions are introduced.

The all-features/raw/event-stream workspace matrix and original 100% plus zero
uncovered thresholds remain intact. The helper command requires complete coverage;
its original export may be retained as helper.json even on failure, but the
nonzero phase still makes metadata.passed false. Thus report availability is not
acceptance. Helper completeness also requires a frozen measurement wrapper and
rejects nested workspace wrappers or a fork hook. Native C, other platforms,
branch/MC/DC and Python tooling remain explicitly outside this Rust command's
coverage claim. Darwin-only helper collection currently prevents claiming this
canonical path as a cross-platform gate.

The separately reviewed collector keeps the actual helper, declared unit and
startup objects, dep-info source owners, external test modules and rust-src
aliases. The prior collector review records its executed evidence and remaining
coverage deficit. This reviewer authored the base pure inventory checker;
coverage_checks.py is therefore excluded from this independent acceptance and
requires its separate reviewer. The outer wiring does not waive that boundary.

## Evidence and remaining gates

The retained pre-follow-up logs establish 152 application tests and eight
integration tests, including the public clock failure before journal cleanup.
The current `work/coverage-closure/full-current-continuous01/workspace-all-features.log`
also records the latest pending-waiter, service-replacement, clock-failure and real
projected-runtime cases passing. The rest of the canonical matrix is still
coordinator-owned; this review does not pre-approve its final result. Mechanical,
performance, complete source-bound coverage and separate adversarial acceptance
remain required. No claim of 100% coverage or a journal performance gain follows
from these tests or the equivalent method-reference cleanup.
