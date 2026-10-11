# Program status: independent coverage audit

Reviewed 2026-10-11. Scope: the stage-one domain implementation in commit
`3490f98`, using `work/osc7501-stage1/coverage/workspace.json` and its metadata.
This is a coverage audit, not runtime or release qualification.

## Measured result

The four measured production files are `report.rs`, `scanner.rs`, `store.rs`
and `value.rs` under `crates/domain/src/terminal/program_status`. The public
`mod.rs` contains no measured executable regions. Test code is excluded from
the following totals:

| File | Lines | Functions | Regions |
| --- | --- | --- | --- |
| report.rs | 139/142 | 15/15 | 251/258 |
| scanner.rs | 67/71 | 5/5 | 88/94 |
| store.rs | 161/164 | 22/22 | 201/206 |
| value.rs | 28/31 | 5/6 | 47/58 |
| Total | 395/408 (96.81%) | 47/48 (97.92%) | 587/616 (95.29%) |

Stage one's new production code therefore does not meet the required 100%
coverage target. This is separate from the wider repository shortfall:
workspace totals are 9250/9889 lines, 957/1036 functions and 13380/14595
regions. Of the workspace's uncovered items, 13 lines, one function and 29
regions belong to these new files; 626 lines, 78 functions and 1186 regions
are elsewhere. This audit does not establish the introduction history of those
other gaps, nor classify them all as defects caused by this feature.

Metadata reports stable sources during the measurement and
`macOS-26.6-arm64-arm-64bit`. SHA-256 comparison of every program-status source
listed in the metadata against `git show 3490f98:<path>` matches exactly.
Correction to an interim review message: LLVM's macro column regions initially
suggested an older `display_text` expression. That inference was incorrect;
the recorded source hashes establish that this artifact covers the committed
stage-one source. Macro region columns alone cannot establish source identity.

## Meaningful missing cases and follow-up

The audit identified the following public behaviors with unexecuted function
or region evidence:

- `value.rs:61`: `display_title` was entirely unexecuted. Exercise absent
  titles and present titles containing direction, formatting, variation,
  shorthand, music and tag characters; verify original text remains intact
  while display text visibly escapes them.
- `report.rs:24,39-42,82`: malformed UTF-8 keys are skipped, malformed UTF-8
  IDs reject the report without addressing root, a last malformed UTF-8 app
  becomes absent, and `blocked:kind=question` is retained.
- `scanner.rs:54`: repeated ESC before a query must preserve detection,
  including every split of that input.
- `store.rs:58`: a pending clear plan owns its identifier and does not change
  the record set before commit; its reported allocation remains bounded.

Four independently authored public tests now cover those behaviors in
`crates/domain/tests/program_status_coverage.rs`. The targeted command
`cargo test -p pty-runtime-domain --test program_status_coverage` passed all
four. These additions strengthen existing behavior coverage; no production
defect or failing-before-fix claim is made. They have not yet been included in
a fresh coverage measurement, so the table above remains the measured result.

`store.rs:133,162` also contains unexecuted revision/generation exhaustion
outcomes. Reaching them requires an impractical number of public operations.
Do not expose private counters through the production API, exclude these lines,
or change runtime behavior to manufacture coverage. At this audit's first
measurement they remained explicit coverage gaps; the local boundary-test
follow-up below subsequently addresses them without exposing a production hook.
Unexecuted defensive code alone is not evidence of a production defect.

## Instrumentation limits and acceptance

The exported artifact reports zero instrumented branch and MC/DC counters.
Neither result means 100% branch coverage. Function instantiations occur in
multiple compiled artifacts: a zero-count region in one instantiation is not
an uncovered source behavior if another instantiation executes it. In
particular, DCS/string cancellation paths have coverage in other instances;
they must not be described as wholly untested from one function entry.

This Rust measurement excludes native C, the upstream terminal dependency,
other platform configurations and Python/build tooling. The guardian helper
is measured separately and is outside this new-domain-file audit. Allocator
metadata and allocation failure behavior are not established by line counts.

No new P1/P2 production finding was established by this audit. The explicit
100% readiness requirement remains unmet for both the new domain code and
the repository. The preceding DDD correctness pass does not waive that gap.
Re-measure after the targeted tests and stage-two changes are frozen, and
record remaining gaps rather than claiming full readiness or release status.

## Stage-two coverage follow-up

The subsequent `work/osc7501-stage2/coverage/workspace.json` measurement reports
421/427 lines (98.59%), 51/51 functions (100%) and 632/640 regions (98.75%) for
the four production domain files. Report parsing and display values are fully
covered in that artifact. Store exhaustion guards account for two uncovered
lines/regions. Scanner summary reports four uncovered lines and six regions,
but exported scanner segments contain no zero-count counted segment; merging
function regions across instantiations likewise leaves no zero-count scanner
region. This is an instrumented-summary discrepancy, not proof of four known
unexecuted source statements. The raw summary remains reported without waiver.

Following explicit authorization for module-local boundary setup, added two
`#[cfg(test)]` tests inside `store.rs`. They place an ordinarily populated store
at generation/revision exhaustion and assert typed rejection, unchanged records
and generation, and preservation of a pending scanner continuation. No runtime
API, configurable counter or production constructor was added. These arithmetic
boundary tests validate failure atomicity without attempting 2^64 operations.

Added a public integration test covering DCS, SOS, PM and APC opacity, their
terminator or CAN/SUB cancellation, and all input split positions. This checks
that embedded reset/report bytes cannot alter records while subsequent visible
reports still apply; it also exercises the separate integration instantiation
of the scanner paths previously exercised by unit tests.

Both counter tests and all five public coverage tests pass. Domain all-target
Clippy with `-D warnings` passes after moving the local test module to the file
end and using a byte-string introducer table. No production behavior changed.
Fresh measurement is still required before reporting new coverage percentages.

## Final measured inventory

`work/osc7501-stage2/coverage-final/metadata.json` reports
`sources_unchanged: true` and `passed: false`. The false result is the mandatory
100% coverage threshold failure, not a changed-source measurement. The final
workspace is 93.8234% lines, 92.5234% functions and 91.9648% regions. The separate
guardian helper is 13.5995%, 19.2661% and 16.0510%, respectively. Whole-repository
readiness remains blocked by that explicit coverage target and the previously
listed instrumentation/platform limits.

The four domain source-file totals are now 466/467 lines, 54/54 functions and
718/722 regions. **These are file totals, not pure production-code totals:**
`store.rs` now also contains the local `#[cfg(test)]` boundary module.

| Domain file | Lines | Functions | Regions |
| --- | --- | --- | --- |
| report.rs | 142/142 | 15/15 | 258/258 |
| scanner.rs | 74/74 | 6/6 | 100/100 |
| store.rs, including local tests | 219/220 | 27/27 | 302/306 |
| value.rs | 31/31 | 6/6 | 58/58 |

The scanner discrepancy reported in the earlier run is absent in this final
measurement. After merging equal source-coordinate regions across compiled
instances, the remaining zero-count store coordinates are line 270 columns
17–25 and line 289 columns 17–25, both `matches!` expressions inside the new
counter tests' assertions. Their tests passed; the unexecuted alternative is
the assertion's mismatch/failure path. The file-summary counts include macro
and instance aggregation and should not be relabeled as four uncovered
production branches. No remaining zero-count production store region was
identified in this coordinate inspection, but branch/MC/DC instrumentation is
still absent; no 100% branch claim follows.

The two newly added application files retain genuine unexecuted defensive
paths:

| Application file | Lines | Functions | Regions |
| --- | --- | --- | --- |
| projection/output.rs | 100/106 | 8/8 | 150/162 |
| projection/program_status.rs | 134/137 | 11/14 | 184/199 |
| Total | 234/243 | 19/22 | 334/361 |

In `output.rs`, line 38 is the absent-terminal fallback and lines 59–63 handle
failure while committing protocol state or recording the processed boundary
after native feed. In `program_status.rs`, unexecuted regions include poisoned
mutex recovery (line 68), scan-admission/scan error conversions (lines 107 and
121), and closed-store or domain-error conversion expressions during setup,
read, commit, exit and drain. Some unexecuted functions are compiler-counted
error-conversion closures, not missing public API methods. These remain measured
gaps; this audit neither removes the paths nor fabricates failure hooks solely
to satisfy a percentage.

The final inventory is frozen for reporting. There are no further code or test
changes from this audit. A mechanical-gate pass and independent correctness
reviews do not waive these coverage gaps or establish release qualification.
