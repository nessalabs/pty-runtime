# Initial fixture failures (not production regressions)

The first guardian test command exited 101. The initial assertion expected Eof
but received `Some(Truncated)` following guardian SIGKILL. ADR G1-04 requires the
explicit drain outcome, not a promise of EOF after supervision failure. The
final test accepts Eof or Truncated while requiring supervision failure, no
fabricated workload exit after guardian loss, victim cleanup, and surviving I/O.
The survivor had already returned the correct response with terminal CRLF. The
initial test looked for LF-only and subsequently used an incorrect fixed marker
length. The second failed run is preserved verbatim in
`guardian-isolation-fixture-error.stdout.txt` and `.stderr.txt`. The final test
uses the actual payload marker's length and does not assume terminal newline mode.

The first event decode command exited 101 during test setup, before any decoder
assertion: `InvalidConfig("page limits exceed runtime budget")` at
`tests/event_stream_decode_contract.rs:29:20`. The page request now uses the
same 1 MiB budget as the established pinned-store integration tests.

The first parked-transfer cancellation command exited 101 because it expected
zero request reservations while still retaining the completed close waiter.
Raw output is in `transfer-cancellation-fixture-error.stdout.txt` and `.stderr.txt`.
The final test explicitly requires one reservation while that waiter survives
and zero after its Drop. This preserves the existing caller-retained ownership
contract rather than permitting a leak or changing production code.

These are test-development failures. None establishes a product defect or a
production red/green TDD cycle; the final valid assertions passed existing code.
