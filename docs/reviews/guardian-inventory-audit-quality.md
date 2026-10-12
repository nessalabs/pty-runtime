# Guardian inventory auditor: independent quality review

## Verdict and scope

**Scoped PASS for the frozen retrospective audit; no coverage acceptance.**
Reviewed `work/counter-integrity/guardian-inventory-audit/audit_guardian.py`,
`checks.py`, `test_checks.py`, `EVIDENCE.md`, `freeze-manifest.json`, and
`frozen-02` artifacts independently of the author. No P1/P2 was found affecting
this retained evidence audit. The original guardian driver remains failed with
exit 1. No production, measurement implementation, test fixture, export,
threshold, or source exclusion was changed. No build or test was run by this
reviewer; the negative-test execution evidence was inspected.

The separation between I/O orchestration and pure inventory/profile checks is
cohesive. The checks do not depend on the command runner; `Inputs` owns content
identity checks, while dep-info, alias ownership, and counter conservation are
separate operations. Direct functions and a small input-binding object fit this
one-off diagnostic tool; an additional framework or strategy hierarchy would
not improve this scope. The fixed repository-relative location and pinned LLVM
text format are explicit constraints, not a general-purpose audit API.

## Evidence independently checked

All 82 entries in the freeze manifest and all 155 retained/current input hashes
in `frozen-02/audit.json` match. All 36 profile-read commands bind their input
profile hashes, successful return codes, empty stderr and output dump hashes.
The 35 raw paths exactly equal both the current on-disk profile set and the
recorded original merge inputs. Using a separate parser over the retained text
dumps, the reviewer recomputed 174 raw function-name/hash identities, 158 merged
identities, 16 omitted identities containing only zero counters, and 2,455
physical counters. Every retained merged counter equals its raw sum.

The source population agrees with the preceding independent guardian ownership
audit: the selected helper/unit dep-info union has 16 project files, including
both compiled test modules. The three extra filenames are retained at their
actual guardian-owned macro region records: 6 alloc, 230 core macros, and 24
core panic records. Their zero file-summary denominators do not hide the region
counters. Linux discovery is explicitly unmeasured, and the supporting fixture
is inventoried without pretending its source belongs to the selected export.

The saturation walk checks function, region, file-segment and branch counts,
rejects unsupported MC/DC data, and distinguishes function hashes from counters.
The saved audit reports 12,135 inspected export count values with no saturation;
this is a necessary sanity check, not semantic validation of source coverage.
Full original totals remain 1,430/1,682 lines, 113/121 functions and 3,438/4,476
regions. The 14 saved static checks include positive cases and focused negative
cases for inventory loss/addition, ownership, sparse merge loss, malformed text,
wrong sums and saturation. They support these narrow invariants; they do not
simulate compiler behavior or establish missing-process profile completeness.

## Finding, disposition, and remaining limitations

**P3 — Directory membership is checked only at the first scan.**
`Inputs.verify()` rehashes all known files at the end, but does not re-enumerate
the source snapshot, original evidence directory or profile directory. A file
added after an initial set comparison would not invalidate that in-flight audit.
This does not change the reviewed frozen result: the reviewer independently
re-enumerated its source/profile populations and checked all saved identities.
Before making this a reusable auditor for directories that may still change,
record each directory population and verify membership again at finalization,
with a negative check for a late addition. No code fix was requested for this
frozen one-off result; re-review of such an integration fix remains pending.

Other limits are correctly disclosed: hashes of retained binaries/profiles and
installed alias sources were collected retrospectively, not before the failed
original run. Installed rust-src manifests are not authenticated archive-content
proof. Equality with the on-disk merge population does not prove every expected
process emitted a profile. Conservation and zero saturation do not establish
correct source-to-counter semantics. Neither this auditor nor this review closes
the independent compiler-mapping findings or turns the original failure into a
passed coverage gate.

## V2 re-review: P3 closed

Reviewed the separate `guardian-inventory-audit-v2` delta and frozen result.
`Inputs` now snapshots directory entry names and types, then re-enumerates every
watched population at final verification. Watches cover the original evidence,
copied source tree, current source/test/script trees, and raw profile directory.
Observer outputs remain outside those populations. Four added static checks
cover late additions, deletion, type replacement, and independent output files;
the retained log reports all 18 checks pass. The earlier 14 checks are unchanged.

Independently verified all 83 V2 freeze hashes and 155 audit input hashes. All six
recorded directory populations still match their frozen snapshots, with counts
30, 33, 14, 4, 6 and 35. The previous P3 is closed for the specified initial/final
drift check. It does not claim filesystem locking, detection of transient changes
restored between observations, or protection after final verification; the V2
limitations now state this. No new P1/P2 was found. The scoped retrospective
verdict remains PASS and coverage acceptance remains false. No build or test was
rerun by the reviewer.
