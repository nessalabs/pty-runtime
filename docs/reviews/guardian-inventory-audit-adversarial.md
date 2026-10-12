# Guardian retained inventory audit: independent adversarial review

Reviewed the additive ignored prototype in `work/counter-integrity/guardian-inventory-audit`, including `audit_guardian.py`, `checks.py`, `test_checks.py`, `EVIDENCE.md`, `frozen-02/audit.json`, retained command output and root `freeze-manifest.json`.

**Decision: no unresolved P1/P2 findings for this frozen retrospective inventory and necessary counter-sanity audit.** This decision is not acceptance of the coverage instrument, the original failed coverage run, a reusable live collector, or a 100% coverage gate. Independent code-quality review remains a separate requirement.

## Evidence and independent checks

- All 82 freeze-manifest hashes and all 155 audit input hashes match their current files. The original source-copy manifest, Cargo artifact selection, selected helper/unit dep-info and complete export were inspected. The selected union contains 16 project source files, including both compiled test modules. Linux discovery is explicitly unmeasured. Supporting fixture, startup test and build source are separately inventoried rather than silently counted as measured.
- Alias acceptance requires an actual per-function nonempty code region owned by a selected compiled primary source and an installed rust-src source. The three aliases retain 6, 230 and 24 actual region records despite zero file-summary denominators. The full measured-file inventory equals selected compiled sources plus these aliases. Duplicate normalized file summaries and missing compiled function owners fail.
- The 35 on-disk raw profiles equal the complete recorded merge inputs. I also checked all 35 have distinct filesystem device/inode pairs. All 36 saved LLVM profile-read commands exited zero, have empty stderr and match their recorded input/stdout/stderr hashes.
- In addition to invoking the authored pure checks, I independently parsed every saved profile dump with a full-consumption regular-expression parser and summed arrays by `(function name, function hash)` without importing the authored parser or conservation function. Result: 174 raw identities, 158 merged identities, 16 omitted identities all zero, 2,455 physical counters and maximum raw sum 309,364. Every merged array equals the raw sum; no raw value or merged sum reaches the saturation boundary. This reproduces the frozen audit.
- The export saturation walk covers function entries, region values, active file segments and both branch sides, including aliases whose file summaries have zero denominators. Reproduced 12,135 values, maximum 314,546, no saturation. MC/DC records fail closed if present instead of being silently ignored.
- The 14 static unit checks pass when run from the prototype directory. They include positive boundaries as well as negative cases; they are not 14 independent end-to-end failure injections. The parser rejects unsupported headers/layout, truncated arrays, duplicate function identities and nondecimal counts. Conservation rejects nonzero sparse loss, unexpected merged identities, mismatched array lengths and changed sums. Inventory negatives cover missing/extra source sets and unsupported or unowned aliases. An initial reviewer invocation from the wrong directory failed module import; rerunning with the correct working directory passed and did not change evidence.

The original driver failure and coverage totals are preserved: exit 1, `Unexpected helper source denominator; inspect inventory`; lines 1,430/1,682, functions 113/121 and regions 3,438/4,476. Branch and MC/DC coverage remain unmeasured. No build, helper/test execution, merge, new profile collection, oracle edit or production edit was performed during this review.

## Remaining limitations

The original run stopped before writing its binary/profile manifest. Current hashes bind retained artifacts retrospectively and cannot establish before/after execution identity. Installed alias source hashes are similarly retrospective; package version/installation metadata is not authenticated archive-content verification. The audit correctly states these limits.

On-disk/merge-input equality does not prove every expected process produced a profile. Zero saturation and conserved merge sums do not prove source-counter semantics. In particular, the separate compiler-instrumentation review remains necessary; this audit cannot close its open semantic findings.

The prototype rehashes known inputs at completion but does not take an atomic filesystem snapshot or re-enumerate directory membership at that final boundary. Consequently its completeness result assumes quiescent retained evidence. A new profile or copied-source file arriving after enumeration could escape the membership check. No such arrival or duplicate physical profile was found in this frozen dataset. Treat this as a reuse constraint: do not promote the prototype to a concurrent collector without sealing/rechecking membership and physical file identity. This does not block the scoped retained-data result.

## Frozen identities

- `audit_guardian.py`: `9aefbb0dddc2877d29a85dd72225aaabbaf468c9436fddc444d51bcad58f673a`
- `checks.py`: `edbf2721e02ecf240039e7b03e5a8e7c587dac33a566648aed32ee60f3fcd490`
- `test_checks.py`: `fdc440f855d67220451628041032d22b11299372a6b36ba63308de19f3a86ec3`
- `frozen-02/audit.json`: `78271c1e069a769b18938491f153ef431f02d46e1aa6261db37cb52688d68068`
- `freeze-manifest.json`: `38ff999991a5b918cff129a0aa63eb396537ff43f13bdf921ed3a92782e29029`

No corrective code change or re-review was needed for this limited audit result. Any tool change or broader acceptance claim requires renewed review against its new evidence.

## V2 membership-drift fix re-review

Reviewed the separate `guardian-inventory-audit-v2` delta and frozen-01 evidence. The V1 sources and evidence remain intact. V2 observes six complete input populations, including recursive source trees, then re-enumerates names and entry types after known-file hash verification. Added/deleted entries and type replacement now fail; generated observer outputs stay outside these populations. This closes the specific V1 missing-final-membership-check concern for observed initial/final drift. It does not create an atomic filesystem snapshot, detect transient changes restored between observations, or prevent subsequent mutation; the V2 documentation explicitly preserves that boundary.

Independently reran all 18 static checks successfully, verified all 83 V2 freeze hashes and 155 input hashes, and re-enumerated all six current populations against frozen records: 30 evidence, 33 copied-tree, 14 original src, four original tests, six original scripts and 35 profile entries. The inventory and counter-check implementation otherwise remains the reviewed V1 implementation. Coverage acceptance is still false and original failure/totals remain unchanged. No new P1/P2 finding for this scoped retrospective audit. All earlier provenance, process completeness, source semantics and unmeasured-platform limitations remain in effect. No builds or measured program executions were run.
