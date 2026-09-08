# Independent capacity-harness DDD and organization review

No P1/P2 DDD, organization or design finding in the reviewed capacity-harness
changes. This is a scoped fixture architecture verdict, not a behavioral or
performance acceptance. No source was edited by this review.

## Boundary and cohesion

`examples/release_load.rs:2` privately owns the support module. The new
`producer.rs:28` is a fixture-local writer over a borrowed descriptor; it owns
pacing, bounded bytes, partial writes and measured polling. `fixture.rs:35`
owns the child PTY setup, separate nonblocking output file description, input
acknowledgements and control records. `population.rs` owns runtime sessions and
control sockets, while `phase.rs:79` coordinates phases, runtime observations
and source totals. `run.rs` emits the report after settling. These are distinct
responsibilities with one concrete owner each, not an unnecessary interface tree.

No domain/application/infrastructure production file changed in this delta.
OS syscalls remain in the independent executable fixture; the runtime is used
through its existing public API. No native terminal pointer, bitmap detail,
parser correction or OS error representation enters domain types. The local
Plan and Outcome types represent actual writer inputs/measurements and do not
mirror a product DTO. A new application port for this fixture would put test
orchestration into product architecture without a demonstrated need.

## CLI and protocol contracts

The CLI distinguishes explicit saturation from idle even though both use rate
zero: saturation requires active producers and zero offered rate. Session,
active-count, chunk, observer and producer-byte limits are validated before
population work. The new byte ceiling is finite, and a capped phase is reported
as censored rather than accepted capacity evidence.

The existing fixed 64-byte frame remains the sole control record outside the
PTY output stream. The seven-value begin record carries the bounded plan; end,
write and done records carry timing, backpressure and cumulative counters.
Writer and parser field positions agree in this reviewed source. The raw payload
stream stays deterministic by absolute offset, so measurement metadata does not
pollute the runtime's byte-accounting path. Readiness remains a separate bounded
blocking handshake before nonblocking control polling.

Direct borrowed RawFd use is limited to the local writer while fixture.rs retains
its File owner. The write/poll unsafe sites state their synchronous buffer and
handle lifetime obligations. Keeping this small mechanism fixture-local is
appropriate; no product-native abstraction or platform-specific runtime fork was
introduced. Behavioral handling of partial writes, saturation gaps and protocol
failures belongs to the independent correctness review, not this verdict.

## Verification boundary

All reviewed support-file hashes match the author's
`docs/verification/load-capacity/source.json`. A fresh size inventory passes:
the largest file is phase.rs at 306 nonblank lines, producer.rs is 155, and every
support file is below 350. The manifest is retained at
`docs/verification/load-capacity-organization/source.json`.

Inspected the retained five passing focused tests and clean Clippy log under
`docs/verification/load-capacity`; these were not rerun by this reviewer.
No full workload, Linux process, native corpus or build was started during the
active soak. Root owns the full gate and behavioral/performance qualification.
