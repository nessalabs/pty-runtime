# Stacked pull request merge assessment

Assessment on 2026-10-12, before publishing the local failure-injection follow-up.

PR #25 (`3490f98ac9282ebe36a496121cbf40adc6474eba`) is open and conflict-free,
with successful hosted checks, but GitHub reports BLOCKED. Main requires
conversation resolution; two current review threads remain unresolved. An
independent read-only review confirmed both against this exact head:

- **P2 — malformed duplicate app pairs:** `report.rs` assigns `app` before
  validating the protocol's general value alphabet. For
  `state=done:app=earlier:app=bad!`, revision 0.4 requires skipping the malformed
  pair and retaining `earlier`; the implementation yields absent app instead.
  The acceptance test also incorrectly groups malformed `bad!`/`bad app` with
  syntactically valid but app-invalid `bad,app`.
  [Review thread](https://github.com/nessalabs/pty-runtime/pull/25#discussion_r4245675257).
- **P2 — interrupted benchmark cleanup:** the benchmark starts a new process
  session but catches only timeout. `KeyboardInterrupt` can bypass process-group
  termination and reaping.
  [Review thread](https://github.com/nessalabs/pty-runtime/pull/25#discussion_r4245670952).

The two source files match the local descendant exactly. Previous scoped review
passes do not resolve these findings. Neither was silently marked resolved.

PR #26 (`f51e97269960c484d15efc17d90ef2323a3ae663`) is open, conflict-free and
has successful hosted checks and no review threads. Its base is #25's branch,
so it inherits both findings. GitHub's CLEAN state does not establish compliance
with the repository's independent-review and coverage gates.

The latest local follow-up has passing mechanical, independent scoped reviews
and scoped performance checks. Its strict coverage target still fails, as
recorded in `docs/verification.md`. Those changes do not fix the two inherited
PR #25 findings and must not be presented as doing so.

No PR was merged, no review conversation was resolved, and no coverage or review
gate was waived. The follow-up is published separately as a draft stacked on
#26. Merge the parents in dependency order only after their blockers are fixed,
affected checks pass and independent re-review confirms resolution.

## Reviewed corrections propagated on 2026-10-12

PR #25's findings are now fixed by addeb58 and 2ead4a2, independently re-reviewed with retained RED/GREEN evidence and a passing 165.727s mechanical gate. Both review conversations were resolved only after the code and evidence were published. The new head a8b21fa has passing hosted Linux/macOS/MSRV and PTY checks. Historical unresolved-finding statements above describe the pre-fix assessment.

Ordinary branch merges propagated those fixes to #26 and #27 without force-pushing. Fresh #26 CI exposed an old dependent expectation that malformed UTF-8 app erases the earlier app. db2f0af corrects stored/resolved app to exactly `earlier` and strengthens continued-message parsing; no assertion is removed. All 24 targeted domain tests pass; separate quality/adversarial/DDD reviews pass and the corrected #26 mechanical gate passes in 225.778s. The failed CI/local fixture runs remain recorded.

PR #27 head `55174b8` contains the propagated corrections. All hosted Linux/macOS quality, MSRV and PTY experiment checks pass at the published heads of #25 (`a8b21fa`), #26 (`db2f0af`) and #27 (`55174b8`). CodeRabbit skipped the dependent-base/draft reviews; those statuses do not replace the independent reviews recorded here.

The final combined-source mechanical gate passes in 195.998331 seconds with all 361 source hashes unchanged (`work/pr25-blockers/final-stack-gate01`). The complete coverage matrix executes successfully across all feature phases and helper commands, but both strict 100% collectors fail. Workspace coverage is 10,195/10,646 lines (95.763667%), 1,049/1,109 functions (94.589720%) and 23,892/27,264 regions (87.632042%). Helper coverage is 1,871/2,030 lines (92.167488%), 154/155 functions (99.354839%) and 4,732/5,655 regions (83.678161%). Rust branches remain unmeasured (0/0).

The independent integrity audit confirms exact raw-counter sums, preserved profile populations and unchanged source/tool/import hashes. It does not turn missing hits into coverage. Results and the failed strict collector exits remain under `work/coverage-closure/pr25-fixed-stack-coverage02`, with the independent assessment in `pr25-blockers-quality.md`. No exclusions, thresholds or assertions were weakened.

No GitHub PR has been merged. The conditional authorization to merge #25, #26 and #27 in that order remains unmet while the strict coverage requirement fails. #27 remains draft.

### Explicit merge authorization on 2026-10-12

After the coverage limitation was explained, the user explicitly selected
“Merge with the documented coverage shortfall.” This supersedes the earlier
100%-coverage merge prerequisite, not the measurements or the correctness,
review and performance gates. Independent reinspection of the final-source
exports confirms the unchanged assertion-failure and statically false TLS
regions described above. The strict coverage gate remains failed; no assertion,
exclusion, threshold or counter is changed. Full release qualification remains
incomplete. The stack is authorized to merge in order once hosted checks pass.

The documentation-head #27 MSRV jobs initially failed before Rust tests because
Codeberg returned HTTP 503 for the pinned native dependency archive. Failed-job
reruns preserve the original failure history. This is a download failure, not
evidence of compiler incompatibility; successful execution is still required.

### Merge progress after explicit authorization

PR #25 merged first as `a566890cba73ec06b6eae71479a6a5fdb7da0417`,
then #26 was retargeted to main and merged as
`137b0557904c9522bfe816ab3ab84ee4e29df97d`. Both verified checked heads were
unchanged when merged; no administrator bypass was used. PR #27 now targets
main and is ready for review. Fresh review identified the UID-dependent EACCES
fixture and stale direct-counter decision heading; both are corrected with
independent review and a new passing full gate. See the final PR 27 correction
section of `docs/verification.md`. PR #27 must still pass hosted checks and have
its corrected review conversations resolved before its authorized merge.
