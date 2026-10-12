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
