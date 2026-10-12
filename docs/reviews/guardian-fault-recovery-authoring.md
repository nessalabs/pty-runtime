# Guardian test-only failure recovery

The user authorized minimal test-only OS failure injection to exercise remaining recovery paths. This change adds three `cfg(test)` checks at the external-operation entries for discovery, anchor creation, and successor handoff. Ordinary helper builds contain neither the fault module nor these checks. No production feature, environment switch, public configuration, runtime field, source exclusion, coverage threshold, or existing assertion was changed.

The `fault_recovery` Cargo integration target uses `harness = false`. The retained verbose compile command proves Cargo supplies `--cfg test` without `--test`; its custom main runs in a genuinely single-threaded process. This preserves the helper's assumptions around fork, signal dispositions, descriptor enumeration, and creation of a controlling terminal. The fixture checks its new SID/group differ from the inherited runner group, then verifies each actual workload's SID, group, and foreground ownership before applying operations.

Fault arms are thread-local, PID-tagged, bounded, non-nestable, and reset by a non-Send scope guard. A copied arm cannot affect a forked child. Every scenario checks that all requested errors were consumed before dropping the guard and allowing successful retry. All control APIs remain in a test-only source file. The dedicated target imports existing helper modules; its dead-code/unused-import lint allowance accounts for other operations and libtest-only declarations imported with those modules. It does not exclude any source from coverage.

Four scenarios use real PTYs and TERM-ignoring shell workloads:

- Discovery failure returns `Retained`, preserves the live owned workload during the retry interval, and later completes real sweep cleanup and verifies SIGKILL wait status.
- Anchor creation failure retains uncertainty until the scan completes, preserves the workload, and later retries real anchor creation and cleanup.
- Handoff creation fails before any socket/fork ownership transfer. Both existing links still deliver the fault frame, the workload remains owned, and the existing retention/retry operation completes actual sweep cleanup.
- Two unavailable anchors after direct TERM produce no false group acknowledgement. The cancellation component reports the exact remaining grace and subsequently applies a real SIGKILL escalation with a single acknowledgement and actual child reaping.

The handoff scenario exercises the failed operation and the existing caller-facing retry operations; it does not run a successful successor transfer or establish that every individual socket/fork errno path executed. The cancellation scenario tests the component's public operations; the existing whole-helper probe separately owns guardian event-loop behavior. These limitations must remain explicit when interpreting coverage.

`work/helper-fault-recovery` retains the initial compile-signature failure, its corrected focused pass, the custom-main compile proof, all-target test output, Clippy output, production build output, and current source/binary hashes. The initial failure was a fixture mismatch with macOS `openpty` mutable optional-pointer types, fixed using null mutable pointers. It was not a product defect. Current validation passes 17 existing unit tests, two startup tests, and all four fault scenarios. Clippy with `--all-targets -- -D warnings` passes. The ordinary helper binary has no `fault_injection` symbol in `nm` output.

Independent quality/adversarial review, canonical collector integration, fresh coverage totals, and the full acceptance gate remain separate. This author record does not claim 100% coverage or self-approve the change.

## Source-path repair after first canonical run

The first complete fault matrix passed all workspace feature suites, helper tests, and protocol probes, but the helper's strict source inventory rejected duplicate normalized paths: importing production modules from a test entry under `tests/` emitted both `tests/../src/...` and `src/...` spellings for the same source. The failure is retained in `work/coverage-closure/full-test-fault-matrix01`; it is not a coverage pass.

The custom test entry now lives at `helpers/guardian/src/fault_recovery.rs` and declares the production modules as normal siblings. Cargo still declares the same `fault_recovery` target with `harness = false`. The four cases remain in `tests/fault_recovery.rs` and are called through its `run` function. Their bodies and the runner body were compared with the frozen first-matrix snapshot and are byte-identical except for the runner's name/visibility. All prior fault seam and fixture files remain unchanged. This repair changes source spelling at compilation; it does not merge duplicate summaries, weaken the inventory check, exclude sources, or discard profiles. Focused and fresh collector validation are pending at the time of this entry.
