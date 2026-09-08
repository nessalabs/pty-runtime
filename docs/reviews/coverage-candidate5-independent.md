# Candidate 5 independent coverage audit

No P1/P2 misstatement found in the current coverage README. Strict coverage readiness remains failed. This was a read-only audit of preserved exports, metadata and relevant production/test source; no tests, builds, native investigation or workloads were run.

## Count, scope and identity verification

The decompressed LLVM exports match `summary.json` exactly: both total dictionaries and every per-file summary (89 workspace file records, 14 helper file records). README rounded percentages and covered/total counts are correct:

| Export | Lines | Functions | Regions | Uncovered lines/functions/regions |
|---|---:|---:|---:|---:|
| Workspace | 7,801/8,401 (92.86%) | 744/866 (85.91%) | 11,278/12,460 (90.51%) | 600 / 122 / 1,182 |
| Helper unit tests | 168/1,449 (11.59%) | 17/106 (16.04%) | 301/2,212 (13.61%) | 1,281 / 89 / 1,911 |

All 11 recorded phases have exit 0 except `workspace-report` and `helper-report`, each exit 1. The report commands enforce 100% lines/functions/regions and zero uncovered counts. All three workspace feature-matrix runs and the separate helper-test run exited 0; report generation retained JSON despite failing strict thresholds. This is a coverage-threshold failure, not evidence that those test phases failed.

Coverage metadata records `sources_unchanged: true`; its 293 source path/hash entries equal the candidate-5 release-build metadata dictionary exactly. The build revision is `71c1d6f42bbd1c547f747fdd7d20ed69e3000c4e`. This verifies matching recorded source inventories; the instrumented coverage image and ordinary release image are distinct builds, not claimed binary-identical. Metadata preserves the fresh coverage build/profile root and uninstrumented helper SHA.

The workspace denominator includes test code, so it is not production-only. The separate helper export covers its unit-test execution, not all real-process scenarios. Both exports record zero branch and MC/DC counts, correctly described as unavailable rather than 100%. Native C/upstream dependencies, other platforms and Python/build tooling remain outside this measurement. The README properly limits the historical percentage comparison because sources/denominators changed; that comparison is not causal test attribution. It also discloses unmerged fallback profiles and the separate helper functional-profile limitation. This audit does not validate or merge those separate profiles.

## Meaningful uncovered behavioral contracts

The following four test gaps are grounded in zero-count region entries in production `crates/application/src/projection/io.rs`, whose bytes still match the coverage source manifest. They are application orchestration contracts testable with the existing replaceable-port fixtures; none requires native engine work. These are proposed missing assertions, not demonstrated production defects. Existing test source was inspected to distinguish nearby coverage from the specific missing behavior.

1. **Reject an invalid checkpoint before parking loses the live model.** Lines 147 and 150 are uncovered: a checkpoint adapter returns the wrong descriptor or a vector whose capacity exceeds the admitted plaintext bound. Add provider variants and assert `InvalidConfiguration` versus `Capacity`, no protector/store call, no live-model loss, no transient memory/disk lease leak, and a valid subsequent retry. Existing `terminal.rs` always returns the requested descriptor and a normal cloned vector; `io_faults.rs` tests malformed **opened** checkpoints after reading, not this pre-protection boundary.
2. **Reject malformed protection results before publishing storage.** Lines 176 and 183 are uncovered: protector failure, or a returned envelope with a wrong key/descriptor, empty ciphertext or ciphertext larger than its promised bound. Assert truthful parking failure, zero commit calls, retained authoritative resident state, released attempted reservations and bounded retry. Existing provider supports a generic failure flag, but `cleanup.rs` turns it on after parking to test authentication during restore; its invalid-reference/panic cases instead exercise storage publication uncertainty. The fixture has no malformed-protect-result variants.
3. **Temporary restore/read budget pressure preserves the only saved source and resumes.** Lines 52–56 and 69 are uncovered: shared checkpoint-buffer exhaustion and restore resident-budget exhaustion before I/O submission. Hold the relevant shared quota in another owner, request transfer/restore, and assert no read/restore call before admission, correct transfer Capacity completion where applicable, retention of source and queued unapplied output, release of partial leases, and ordered recovery after budget release. `pressure.rs` tests parser capacity and resident checkpoint pinning; `io_faults.rs` tests I/O-executor rejection after resource acquisition. Neither asserts these pre-I/O quota-denial paths.
4. **Rejected cleanup-job admission must preserve the exact garbage source for retry.** Lines 245–246 are uncovered: delete submission fails and the source is requeued. Arrange a stale checkpoint pending deletion, reject the blocking executor, and assert no store deletion occurred, the same reference/reservation remains charged, live output still progresses, and enabling admission later deletes exactly that object and releases its charge. `cleanup.rs` tests rejected **parking** submission, permanent store-delete failure, and late commits during close; those do not assert this ordinary garbage-delete executor rejection path.

These four tests would prove outcome and ownership contracts, not merely mirror constructors, debug output or isolated counter updates. They should preserve failing evidence only if they expose a behavior defect; no production change is justified by this coverage audit alone. Existing strict coverage and the full release requirements remain unmet until independently remeasured.

## Reviewed artifact identity

| Artifact | SHA-256 |
|---|---|
| `docs/verification/coverage-candidate5/README.md` | `a02c55e7a75280eb361900d1572941cbbf19ea56efc971931ce562f9426d5f18` |
| `docs/verification/coverage-candidate5/metadata.json` | `8e8baf79fb52648fb72eaedc9379aae2724bcf9b2b6ff591a504fd1956f18e5a` |
| `docs/verification/coverage-candidate5/summary.json` | `d7e248f0cc0109bd4a02f4394ad26bd8a5967e12d8f4034dfbc1b3628b8bd0fa` |
| `docs/verification/coverage-candidate5/workspace.json.gz` | `ce82287b95133cf495b4f095231131a1c6ec6ae50d1008ab77a5e1089af641fb` |
| `docs/verification/coverage-candidate5/helper.json.gz` | `5d4c9c49693d3de5fe37d11a09ca76aab019e3220195302403fe21f9462ded74` |
| `docs/verification/release/candidate5-build/metadata.json` | `7e1c36d0bcc14a8ae0ec7a1e5d58353758f23266733f11ae619fa0f88b8387c5` |
| `crates/application/src/projection/io.rs` | `eeb7106d8656466199a3e63269443cfdd2d1b42a45ba91c67c6f0b30b8a66823` |
