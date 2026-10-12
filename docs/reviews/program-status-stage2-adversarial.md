# Program status stage two: independent adversarial review

Reviewed 2026-10-11 against stage-one commit
`3490f98ac9282ebe36a496121cbf40adc6474eba` plus the stage-two working tree.
The final source identity from `scripts/performance.py::identity` at re-review is
`b5284b7163a2d482efb33d27394327874056c58cffb91ae6596b08d1f47744f8`.
The initial scoped review identified
`d2f25a26befc42f19780694ff55bdda411092c6cbafa3bcdd898dc12d5fa9199`; the later
changes add real-test assertions and correct a stale worker comment, without a
production behavior change.
This identity includes production sources, fixtures, tests and measurement
scripts; the parent commit alone does not identify the uncommitted integration.

**Scoped result: PASS. No unresolved P1 or P2 production finding was found in
this stage-two review.** This is an adversarial review result, not a substitute
for the final mechanical gate, coverage target, other specialist reviews or
release qualification.

## Production paths challenged

The review inspected `projection/program_status.rs`, `projection/output.rs`,
`projection/worker.rs`, `projection/queue.rs`, admission, coordinator creation,
teardown, runtime exit forwarding and the public snapshot API, plus the domain
scanner and snapshot reservation additions.

- A native feed can mutate before returning an error. The implementation fails
  the projection after any slice error, never retries the accepted prefix,
  never advances the original chunk cursor, and commits the protocol plan only
  after every slice succeeds. Failed snapshots report the projection error.
- Queries split native feeds at their terminators. The fixed protocol reply
  follows native effects from that slice and precedes effects from later
  slices. Aggregate replies have one logical limit, while simultaneous native
  and aggregate capacities are admitted before feeding. Discarded reply bytes
  are cleared and no partial aggregate reaches the process writer.
- Scanner scratch, persistent records and copied snapshots have distinct
  reservations. Capacity pressure before feeding leaves the model and committed
  scanner untouched; a reservation that cannot fit the configured total fails
  instead of retrying forever. Ordinary ground-state text retains no protocol
  heap allocation. Snapshot ownership keeps its charge after session close.
- Exit has a dedicated preallocated queue slot, survives exhausted reader and
  request admission, and follows previously admitted output. Final drain
  reapplies cleanup to output buffered after exit. Duplicate notifications do
  not repeatedly change the record revision. Drain-before-exit is covered.
- Records and their revision survive native parking and restoration. A parked
  exit marker does not require resident capacity or restoration. Snapshot reads
  complete while a native checkpoint is blocked, demonstrating that they do not
  wait on the native workspace lock.
- Native checkpoint compatibility and raw byte ownership are unchanged. Raw
  sessions do not acquire automatic status observation or query replies.

## Independent tests and evidence

The reviewer added the test files
`crates/application/src/projection/tests/program_status.rs` and
`program_status_terminal.rs`, and registered them in the existing test module.
The fault adapter implements the external `ITerminal` boundary and delegates
real model mutation before injecting `HistoryIncomplete`; it does not add a
production test hook or reproduce the protocol parser.

The 14 independently authored acceptance tests cover:

1. Both query terminators, native feed splitting and reply order before DA1.
2. A query spanning output chunks, exactly one reply and silent normal reports.
3. A mutating second-slice failure, no replay, no cursor advancement and no reply.
4. Repeated queries at the exact aggregate limit and native replies exceeding it.
5. Ordered exit with local or shared parser and request admission saturated.
6. Buffered reports after exit, final drain and drain-before-exit cleanup.
7. Parked snapshots and exit cleanup with native capacity fully occupied.
8. Snapshot reservation lifetime beyond session close and redacted Debug output.
9. Protocol admission pressure followed by one successful retry.
10. An impossible protocol memory budget failing before native feed.
11. Lazy protocol allocation for ordinary output.
12. Record-cap configuration boundaries.
13. Native restoration preserving records and revision.
14. Snapshot completion while native checkpoint encoding is blocked.

Evidence retained locally under `work/program-status-application-acceptance/`:

- `before.log`: compilation failed because the new integration API was absent
  or incomplete. This is missing-feature evidence, not a claimed behavioral
  defect in the completed implementation.
- `implemented-first.log`: all 14 tests passed once the integration compiled.
- `after.log`: all 14 passed after strengthening the aggregate-reply test with
  its exact-limit successful case.
- `clippy.log`: strict application test Clippy passed with `-D warnings`.

No stage-two production defect was discovered by these tests, so there is no
claimed stage-two behavioral failure/fix/re-review sequence. The three real
stage-one parser defects and their failing/passing evidence remain recorded in
the stage-one review. The parent will run the complete gate against the final
frozen source identity; these focused test logs do not replace that run.

## Real-runtime evidence and remaining limits

All three tests in `tests/program_status_runtime.rs` passed in the parent-run
`/tmp/pty-program-status-runtime-final.log`, which this reviewer read during
re-review. This reviewer inspected their assertions and the real-child fixture. They cover
an authoritative query reply before Ghostty DA1, byte-identical raw replay,
passive raw-session behavior, reported records and exit cleanup, and real native
parking/restoration. The strengthened projected-session test compares the full
Ghostty view against an independent terminal fed only `STATUS_READY`; this
checks that the reports, detection query and DA1 request have no display effect.
It also starts a transfer before the final child output and asserts that all
continuation output bytes equal the exact original `TAIL`, including its OSC
7501 sequence, through a failure-free end marker. The parent also reports that suppressing the protocol reply
made the child-handshake test fail, then restored the production source. Those
runs and mutation evidence are parent-owned, not independently rerun here.

The deterministic tests prove application orchestration through the terminal
port; they do not independently establish Ghostty parsing or operating-system
scheduling. The real tests do not force one specific kernel read boundary at
exit; the deterministic ordered-marker/final-drain tests establish that case.
BEL-terminated query detection and split terminators are primarily covered by
the domain/application tests. The final assertions close the initially recorded
real-Ghostty view-equality and protocol-specific transfer-byte gaps. There is
still no new protocol-specific event-stream byte-equality assertion in the
inspected real-runtime suite. The re-review found no additional P1/P2 issue;
the scoped PASS is unchanged.

Reservation tests and code inspection concern admitted allocation bounds, not
allocator overhead, OS memory peaks or native RSS. Performance results, complete
coverage and platform inventory, 10,000 lifecycle cycles, 100,000 attach/detach
operations and a 12-hour soak remain separate evidence. This review makes no
full runtime readiness or release qualification claim.

## Coverage-driven acceptance follow-up

The first stage-two coverage artifact reported `output.rs` at 100/106 lines,
8/8 functions and 149/162 regions, and `program_status.rs` at 131/137 lines,
11/14 functions and 180/199 regions. The reviewer inspected the merged segment
and function data in `work/osc7501-stage2/coverage/workspace.json`, rather than
treating every zero-count feature instantiation as a separate behavioral gap.
In particular, snapshot Debug redaction already has a passing test and is
covered in the merged file; one zero-count instantiation does not establish
that the formatting behavior was untested.

Five additional public coordinator acceptance tests now cover reachable
boundaries in `projection/tests/program_status_boundaries.rs`:

- Snapshot requests refuse both closing and closed sessions.
- Snapshot quota refusal leaves the stored record and quota ownership unchanged.
- A scan can fit while its required persistent record reservation cannot; this
  fails before native mutation and releases scratch admission.
- Failure to acquire either record or reply memory after admitting scan scratch
  rolls back all provisional charges. The subsequent retry applies the report
  and replies once.
- A potentially mutating failure in the slice that ends a query does not feed
  later slices, advance the cursor, or deliver a partial reply.

All 19 application protocol tests passed in
`work/program-status-application-acceptance/coverage-boundaries.log`; strict
Clippy passed in `coverage-boundaries-clippy.log`. The source identity observed
when this follow-up completed was
`aaf8eabeb3597197fedffaebba148c631566499d77df1ca81de634df0b62bf8f`.
Only tests, their helper visibility and module registration changed. No
production behavior or test-only production hook was introduced. No new defect
or P1/P2 finding resulted; the scoped adversarial PASS is unchanged. The parent
will rerun the complete gate and coverage after all coverage follow-ups settle.

The remaining uncovered defenses must not be disguised with invalid private
state. The public coordinator validates chunk and record limits before these
helpers, the serialized worker owns scan/commit, and native workspace ownership
excludes cleanup during a feed. Missing-terminal dispatch, invalid internal
record limits, overlarge internal scan chunks and a stale internal commit are
therefore not ordinary reachable public operations. Generation/revision
exhaustion is practically unreachable through public calls and remains an
honest measurement gap. Poison recovery likewise has no supported public panic
injection while the protocol mutex is held. The snapshot's missing-store guard
can protect a genuine close/read race, but no deterministic external seam
forces that precise interleaving; this follow-up does not claim to cover it.
These assessments do not remove any code from the coverage denominator or
replace the requested 100% target with a lower threshold.
