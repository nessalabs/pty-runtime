# Final census race: independent correctness and organization review

Reviewed 2026-09-09. Scope: scripts/release/load.py final-census sampling guard and scripts/tests/test_load_final_census.py, with retained RED/GREEN logs. Applied repository standards. Read-only review; no tests, builds, loads or implementation edits by this reviewer.

## Result

No P1/P2 correctness or organization blocker found in the selected fix. It ends periodic sampling after the acknowledged closed census while preserving completion/exit validation and existing checkpoint resource assertions. The retained historical resources-projected-1 failure remains a failed trial; this fix does not retroactively qualify that group or explain the separate matrix driver interruption.

## Production path

The only behavioral change is the additional condition `'closed' not in checkpoints` before periodic census. The closed checkpoint is sampled and recorded before its acknowledgement. Its existing requirements—exactly one remaining process and no zombies—are asserted before writing continue. Any sample/assertion/acknowledgement failure still enters the existing failure/cleanup path. Although checkpoints receives the snapshot before the assertions, assertion failure exits the try block, so it cannot bypass rejection through the new condition.

After successful acknowledgement, the runtime is allowed to exit while its complete output remains queued. A second periodic census is no longer semantically useful and can race that normal exit. Suppression applies only after closed: earlier missing processes/sample errors remain failures. Explicit later checkpoint events still follow their existing sampling path; the guard does not suppress all census failures or swallow exceptions.

The driver continues draining stdout to its end marker, waits for the actual child exit, joins the reader, closes pipes and asserts both exit0 and complete. It still accesses measurement_start/measurement_end for CPU accounting and independently emits target results. This patch does not convert a missing complete marker, nonzero exit, absent measurement checkpoints or a failed closed resource assertion into success. It does not add a new overall checkpoint-order validation contract; it preserves the existing trusted fixture protocol.

## Regression and controls

The fixture runs a real temporary Python child that emits checkpoints and waits for driver acknowledgements, then emits optional complete and exits with the selected status. Census/clock are controlled test doubles. The poll wrapper waits for actual child exit after the final acknowledgement but returns the prior live observation once, deterministically placing termination before the next census. The sample double rejects that periodic census with ProcessLookupError. This targets the poll-to-census race and queued completion output; it is not a real OS census integration test.

The positive test requires exactly measurement_start/measurement_end/closed samples, complete, trial_result and closed pipes. Its old-driver RED log has the expected failure after closed with complete queued, child exit0 and no driver kill. The disappearance-before-closed control uses final ready instead, still reaches periodic sampling and rejects it. Additional controls reject missing complete, exit17, remaining descendants and zombies. These checks would fail if the fix indiscriminately ignored sampling failure or equated closed with successful trial completion.

Read the retained red.log: four tests, one intended positive regression failure and three passing controls. Read green.log:22 related census/diagnostics/target tests pass, including the four new tests. The initial-fixture-errors.log is separately retained; it is not production RED evidence. This reviewer did not rerun the suite or independently bind those logs to an execution-time source manifest, so current source hashes below identify the inspected content, not an inferred historical build identity.

## Organization and limits

The production change remains a narrow condition in the existing collection loop with a useful rationale. The test module has one focused fixture supporting parallel scenarios, explicit child ownership and short bounded real-child waits; no runtime/domain implementation or public API changes occur. Private census mocking is appropriate for deterministic race placement, while real pipes/child exit exercise driver orchestration. Retain a fresh complete five-repeat projected-1 case and required mechanical gate separately before accepting new matrix evidence. No claim is made about native correctness, the12-hour soak or full release readiness.

## Reviewed SHA-256

| File | SHA-256 |
| --- | --- |
| `scripts/release/load.py` | `9176921b0e505773b53ad767a01ee559ef2ad099d5f9116905d1bc612c9be5ec` |
| `scripts/tests/test_load_final_census.py` | `039306032f261869136f01aeb23accc788af8f0f6e80d12033f2bd41096954eb` |
| `docs/verification/release/final-census-race/red.log` | `861bb6de91c41302c86fc20f55df1d9b42b0ba8be48cde7d42074368144a8e89` |
| `docs/verification/release/final-census-race/green.log` | `80363981b55655f740cfa82864b83eba22b35439e603cfffeb25083771b3bea1` |
| `docs/verification/release/final-census-race/initial-fixture-errors.log` | `5c0705d17d157882f209933e9791d51163d47a7b954828e16921741c51d32a37` |
