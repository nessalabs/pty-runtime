# Independent event-stream behavioral correctness review

2026-09-08. Scope: optional infrastructure event-stream publisher, wire codec,
portable error mapping, and the three root event-stream integration suites.
The reviewer did not author production code. No full gate was run during
concurrent guardian work.

Disposition: no open P1/P2 findings in the reviewed source after the callback
fixes below. This establishes focused host behavior, not release qualification.

## Resolved P1: cancelling a source wait could deadlock the session

`publish_next` waits through the existing raw `Attachment`. Its cancellation
path cleared a caller-owned Waker while holding `SessionContext.state`. A waker
can own another attachment to the same session. Destroying that waker reenters
observer removal and deadlocks the mutex, blocking both cancellation and PTY
progress. Raw polling also cloned/replaced wakers under that mutex.

The independent real-runtime regression
`cancelled_publication_drops_a_reentrant_source_waker_without_locking_the_session`
reproduced a bounded timeout before the fix. The root moved incoming clones and
old/unused waker destruction outside session locks for raw and completion waits,
and observer removal/clearing. The regression now passes, including cancellation,
process completion and shutdown after the destructor returns.

The analogous projection `Ticket` cancellation path was also confirmed with
`cancelled_projection_wait_drops_reentrant_waker_outside_ticket_lock`. Its waker
destructor completes the same ticket. It timed out before the fix and passes
after the root moved ticket waker destruction outside its mutex.

## Observer callback isolation

The root additionally catches each wake callback panic independently in raw
notifications, transfer journal notifications, and projection ticket completion.
This keeps one observer from suppressing remaining notifications or unwinding
runtime-owned output work. The new real publisher regression
`panicking_publisher_waker_does_not_suppress_another_observer_or_pty_output` proves
that the first registered waker executes and panics, the second is notified,
and subsequent input and output still succeed. The fix was reviewed in source;
this test was run after it landed rather than presented as a pre-fix reproduction.

## Publication state, bounds and receipts

- The only asynchronous suspension points are source read and sink append.
  Before source read is ready, cancellation leaves the raw cursor unchanged.
  After readiness, ownership moves immediately into `Pending::Source` before
  fallible encoding. There is no suspension between consuming the source and
  storing it. A conversion failure therefore cannot skip the consumed source.
- Preparation replaces that source with one immutable prepared event. Sink await
  receives a clone of the exact same ID/schema/payload; all errors and cancellation
  retain the prepared event. The existing actual-store tests prove repeated
  capacity rejection, uncertain commit, malformed receipts and cancellation both
  before and after commit preserve identity and acknowledged byte position.
- Receipt acceptance compares the whole prepared event, exact stream incarnation,
  cursor version and strictly increasing nonzero event offset. PTY acknowledgement
  advances only after those checks. The prior architecture review's monotonic
  offset fix is present and its contradictory completion receipt test passes.
  Offset gaps remain valid because another writer can interleave.
- Successful completion is latched. Repeated raw completion cannot generate more
  records, and completion retries preserve their identity until acknowledgement.
- Runtime page policy caps the source at 65,536 bytes. Publisher preparation can
  temporarily retain that page plus an encoding Vec and copied immutable payload;
  after preparation only one payload remains. The pinned dependency represents
  `Payload` as `Arc<[u8]>`, so append clones do not multiply its byte allocation.
  Each publisher consumes an existing quota-counted observer slot; it has no
  internal queue or retry task. External sink allocations remain sink-owned.
- IDs are fixed-width hexadecimal components (71 bytes), below the pinned
  dependency's 256-byte identifier limit. Sequence increment is checked before
  source consumption and never wraps. Identity exhaustion and allocator failure
  were inspected rather than fault-injected. Cross-process identity is randomized,
  not an exactly-once restart protocol. Dropping a publisher intentionally loses
  uncertain retry state; a new publisher has a new identity.

## Wire and isolation evidence

The codec has bounded input length, checked subtraction for byte ranges, exact
lifetime/range validation during encoding, explicit stable codes for all current
portable errors, and rejects unknown tags, trailing bytes and incomplete completion
facts. Existing tests cover every current nested error category, binary maximum
page, gaps and process/drain distinctions.

The independent completion test constructs external wire bytes without using
the encoder, checks every truncation and selected contradictory/unknown fields,
then performs 12,800 single-byte perturbations. Invalid values return errors;
accepted semantic variations roundtrip canonically. This is bounded corruption
coverage, not a claim of exhaustive multi-byte fuzzing or authentication.

Actual pinned-memory-store tests establish full-store rejection without eviction,
stale-incarnation rejection, subscription reconnect and interleaved store/PTY
cursors. The Ghostty isolation test stalls the sink while projected input, resize,
parser catchup and cancellation finish, then receives the exact retention gap.
Construction creates no stream, file, background forwarding task or persistence
facility. The caller explicitly supplies the sink and stream incarnation.

## Independent execution

- `cargo test --locked -p pty-runtime-infrastructure --features event-stream event_stream`:
  four codec tests passed.
- `cargo test --locked --features event-stream --test event_stream_failures --test event_stream_isolation --test event_stream_reconnect`:
  nine actual runtime/store integration tests passed.
- `cargo test --locked -p pty-runtime-application cancelled_projection_wait_drops_reentrant_waker_outside_ticket_lock`:
  the independently reproduced ticket regression passed after the fix.

The integration tests use the pinned actual MemoryStore plus a controlled sink
wrapper for precise errors, receipts and cancellation barriers. They do not prove
other provider durability, performance under many concurrent publishers, arbitrary
allocator failure, or another platform. No new release/soak claim is made.
