# Independent control-admission DDD and organization review

No P1/P2 DDD, organization or design finding in the reviewed change. The change
uses the existing bounded request-ticket ownership to separate ordered controls
from parser output admission, while preserving one ordered event queue. No source
was changed by this reviewer.

## Ownership and finite queue bound

`projection/admission.rs:12` retains parser byte/slot leases for every nonempty
output. Empty output returns Accepted before constructing or enqueuing an event,
so it cannot exploit the zero-payload path to add uncharged queue entries.

All four zero-payload queue callers first acquire a request ticket through
`ticket()`: resize, view and checkpoint in admission.rs, plus begin_transfer in
snapshot.rs. That ticket owns a paired shared/local request lease. The event
retains an Arc to Ticket, so dropping the observer wait does not release the
reservation while the event remains queued or executing. Completed retained waits
also keep their ticket reservation. Failure/close paths retain their existing
completion/drop ownership. No unbounded or unleased control entry was found.

`coordinator.rs:76` computes staging_slots + request_slots with checked addition
and reserves that finite queue capacity before any event admission. The number
of queued output events is bounded by local parser slots and the number of queued
controls by local request tickets; executing or retained results only make that
upper bound conservative. The existing local bounds and shared reservations are
unchanged. Allocation failure still produces a typed capacity error. No priority
queue, reordered control or bypass of earlier output was introduced.

## Domain and application responsibility

Domain option comments now describe the distinct resources accurately: parser
output slots versus outstanding observation/control request slots. Their finite
validation remains in domain. Application continues to acquire leases and own
queue/event lifetimes at the use-case boundary. It depends only on domain/std;
domain has no external dependencies. No OS/native condition, serialization DTO,
service locator or generic repository was introduced.

The small conditional in the existing staging helper is supported by an exhaustive
inspection of its current callers; it does not require a new public abstraction.
The coordinator's queue-reservation calculation belongs beside owner construction,
and ticket ownership stays centralized in observation.rs. All reviewed Rust files
remain below 350 nonblank lines (largest: coordinator.rs, 222). This is cohesive
ownership separation rather than a file-size-driven split.

## Proof and limits

Inspected the two retained red cases for exhausted local/shared parser slots and
the author's green projection suite (52 pass). The control regressions check
bounded request rejection, exact output/resize FIFO ordering, completed-wait
reservation retention, release and reuse. These tests were not rerun in this
architecture review; the independent correctness reviewer owns broader queue and
abandonment validation.

Source hashes and scoped checks are recorded in
`docs/verification/parser-control-admission/organization-source.json`. Admission
isolation does not by itself prove a latency threshold under a stalled endpoint;
full workload repetition and the final source-bound gate remain root-owned.
No Linux workload, native corpus, runtime mutation or full gate was performed by
this review.
