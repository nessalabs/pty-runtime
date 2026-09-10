# Independent ordered-transfer correctness and DDD review

2026-09-08. Scope: domain transfer ordering and application journal, snapshot,
stream termination, observer/wait, native publication and parked IO integration;
root `Session::begin_transfer` and process-drain forwarding. This reviewer did not
write those production files. Four independent regressions were added under
`crates/application/src/projection/tests/ordered_review.rs`.

Current disposition: two P1 defects and two P2 findings were fixed and independently
re-reviewed. No open P1/P2 findings remain in this scope. No full gate or new platform
qualification is claimed by this review.

## Resolved P1: a successful late native feed could publish beyond immutable End

The original `Journal::append` checked closure and unavailability but not a
previous End. A native feed can be executing when a separate failed scheduler
wake calls `ProjectionCoordinator::fail`. That call seals the existing valid
prefix. If the in-flight native feed then returns success, the old implementation
appended another event and advanced the journal tail. A consumer could therefore
receive a mutation followed by an End whose boundary precedes that mutation.

The author added an End guard to append. The independent regression
`failed_end_stays_immutable_when_an_inflight_native_feed_returns_success` pauses
an actual coordinator feed through an injected terminal wrapper, reports failure
from the other thread, observes End, then releases the successful feed. Reading
the original cursor must still return the same failed End, without a late event.
The test passes after the fix. It proves the concurrent orchestration schedule,
not a native library failure mechanism.

## Resolved P1: waker destruction could reenter a locked journal

The original journal read replaced a registered `Waker` while holding its mutex.
Waker destruction is caller code: a waker can own another observer whose Drop
legitimately deregisters from the same journal. Clearing the first registration
would then deadlock trying to reacquire the mutex.

The author now clones incoming wakers outside the mutex, takes the old
registration, computes all success/error results, and releases the mutex before
dropping old or unused wakers. The independent regression
`replacing_a_pending_waker_drops_its_observer_outside_the_journal_lock` constructs
exactly that ownership chain. Clearing the registration completes within a
bounded two-second channel wait. This is a real destructor/reentrancy proof,
not merely a notification counter assertion.

## Resolved P2: terminal ordering policy was split across layers

Domain `TransferOrder` owns sequence, applied byte/control boundary, and retained
floor. Application `Journal::State` previously owned end/closed/unavailable and enforced
no append after End. The just-fixed P1 concerns a pure terminal-order invariant,
not a storage, allocation or scheduling choice. Per the repository's rule that
domain owns validated transitions, sealing and append permission should live in
the domain transfer policy. The journal should retain payload maps, quota leases,
observer registrations and eviction mechanics. This does not require another
interface or framework. The author moved seal, append/read permission, terminal
state and observer opening eligibility into `TransferOrder`. The journal now
delegates these transitions. Independent source inspection and ten passing domain
projection tests confirm the boundary, including immutable failed End and rejected
late mutations.

## Resolved P2: observation traffic could postpone End indefinitely

`finish_stream` originally required the whole request queue to be empty. Views
and snapshots remain admissible after drain and do not advance mutation order.
A caller admitting one view before every scheduler turn could therefore keep a
fully applied stream Pending indefinitely. The independent regression
`observation_requests_do_not_delay_the_final_applied_prefix` reproduced this failure.
The author now waits only for queued output/resizes, pending resize/reply work and
processed-byte catchup. The regression passes.

The related IO edge was inspected and tested: `finish_stream` runs before the
parked worker's pending-IO early return. The regression
`parked_snapshot_io_does_not_delay_an_existing_observers_end` leaves the provider
read job queued, sends EOF and proves an existing observer receives End before
the read executes. No production change was necessary for this IO case.

## Ownership and boundary assessment

- A journal record owns its byte and slot reservations. Returned `TransferEvent`
  clones share the same immutable allocation and leases. Evicting its map entry
  or dropping the last observer cannot release a consumer-held record's charge.
- Capacity loss advances the retained floor explicitly. A zero-byte resize still
  needs a record slot; losing that control also requires resynchronization.
  The parser continues rather than waiting for an observer to release capacity.
- Snapshot encoding and journal observer opening occur under the same exclusive
  model work serialization. The opened boundary is checked against the actual
  checkpoint's processed bytes and successful control generation.
- A parked transfer reads its saved source without restoring a native owner.
  Subsequent staged mutations wait behind that read, so the observer is installed
  before they are applied. Cancellation and rejected worker admission release the
  provisional observer lease; checkpoint pins and observers can then live and
  release independently.
- `TransferWait` borrows one mutable observer and clears its registration on Drop.
  Explicit cursors do not advance because a wait was cancelled. Other observers
  have independent cursor and registration state.
- Process drain closes output/control admission; End waits for queued mutations and
  pending replies/resizes to finish and processed bytes to catch up. Parser
  failure ends only the known valid prefix. Runtime close reports Closed rather
  than manufacturing a successful End.

The application/domain dependency direction is otherwise appropriate. Payloads,
quota release, wakers and external IO remain in application; portable boundaries,
errors, byte/control/event cursor distinctions and ordering rules live in domain.
A transfer cursor is not a raw byte cursor: two successful resizes at the same
byte position still require two ordered events.

## Independent execution and limits

Executed independently after the concurrency, domain and liveness fixes:

- `cargo test --locked -p pty-runtime-domain projection`: ten tests passed.
- `cargo test --locked -p pty-runtime-application projection`: 33 tests passed,
  including the four new independent regressions.
- `cargo test --locked --test ordered_transfer_runtime`: one test passed after
  concurrent Guardian integration settled. The earlier compilation-blocked
  attempt is superseded by this successful independent execution.

The real test was inspected: two consumers restore a parked Ghostty checkpoint
with unfinished UTF-8 and history, apply original output and two resizes at the
same byte position, suppress replica-generated replies, and compare final models
with the authoritative terminal. This reviewer executed that real test successfully.

No full gate was run because the root explicitly reserved it until concurrent
adapter changes settle. New ordered-observer performance, target-matrix and
release/soak requirements are not established by these focused tests.
