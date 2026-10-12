# PR 25 blockers: independent DDD and ownership review

Scoped PASS; no new P1/P2 dependency-direction or ownership finding in the current
four-file delta on `codex/osc7501-domain`, base
`3490f98ac9282ebe36a496121cbf40adc6474eba`. Reviewed actual parser/runner changes,
independently authored acceptance/orchestration tests and saved red/green logs.
No build, benchmark, test or source mutation was performed by this reviewer.
Final mechanical, performance and coverage gates remain separate.

The report parser owns protocol grammar and report construction within the domain.
App pairs now pass through the same general value-alphabet validation as the
other keys before replacing the borrowed candidate value. Invalid UTF-8 or
malformed value syntax skips a pair, preserving an earlier valid app. A
syntactically valid but semantically invalid app still replaces it and is then
filtered to absence. This preserves the distinction between pair grammar and
field semantics without moving state into the scanner/store or adding allocation.
Hard limits are checked before malformed skipping; malformed id still rejects
the report. No infrastructural dependency or new policy object is introduced.

The independent acceptance cases exercise scan/commit/records through BEL/ST and
every two-chunk split, later state/message processing, duplicate semantics and
32/33-byte bounds. The old conflated expectation was corrected into separate
malformed-pair and invalid-app-semantic contracts. Retained red evidence has the
two intended failures among 19 tests; the final parser passes all 19. This is
bounded protocol evidence, not a weakening of the byte limits or store contract.

Benchmark cancellation ownership stays in the process-launching measurement
runner. Its cleanup helper has one shared deadline and owns group signalling,
both capture-pipe closures and direct-child wait. It does not call communicate
again after a timeout path may already have closed streams. KeyboardInterrupt
at each cleanup operation retries that operation within the remaining budget;
cleanup failures attach as a tuple to the original exception, followed by bare
re-raise, preserving Python 3.9 compatibility and cancellation identity.

The group signal is conditional on Popen's unreaped state; a cached PID after a
known reap is not sufficient group-ownership evidence and is not signalled. This
is valid for the stated Cargo lifetime, where Cargo owns/waits for its benchmark.
It is not a generic orphan-descendant supervisor or a guarantee against competing
reapers/arbitrary launchers. In particular, a previously reaped launcher with
independent surviving descendants is outside that ownership contract. Returning
cleanup diagnostics after a timeout does not establish successful reaping; the
fixed deadline bounds waits/retries, not every OS instruction against unlimited
interrupt storms.

The actual SIGINT regression retains a live launcher/descendant group and checks
both are gone before the fixture's fallback cleanup is released. The fallback
signals its own live group, not a cached descendant group number. The red log
shows a leaked live launcher; green evidence covers actual success, timeout and
SIGINT. Four further mock cases establish cleanup instruction ordering, repeated
interrupt handling, known-reaped no-signal behavior and preservation of original
cancellation plus failures. All seven pass in `interrupt-edge-green.log`; those
mock cases are not presented as extra real-process signal evidence.

The two fixes have distinct owners and remain separate: domain report syntax
versus measurement-process lifetime. No benchmark policy reaches the runtime,
no test fixture becomes production configuration, and no coverage exclusion or
threshold change is introduced. A passed bounded review does not waive previous
strict-coverage failures, final frozen-source checks, performance or PR stack
acceptance requirements.

Reviewed hashes:

- report.rs: `79dbcf2811d6e0386a7cc5bc2dec1a2bd69159d7f50ab38147613e7a82fd99fe`
- program_status_acceptance.rs: `99ace72430ee272720b11fa1606045f79712edf669b675509b6a78f4401bc724`
- program_status_performance.py: `7fff923f914678b39c303755714c8f236f1f7c2ddd5c2e965b430403c77892a8`
- test_program_status_performance.py: `7291b8f2999cdd2524228aed164ce7c72df9f0f8954d6863b3f043283954cfcc`
