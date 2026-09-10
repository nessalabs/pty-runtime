# Archived specialist reviews

One-off DDD, organization, and correctness notes from implementation loops.
Prefer ADRs + the verification ledger for “what is true now.” Use this tree when
auditing *how* a decision was challenged.

## By feature (filename prefixes)

| Feature | Typical prefixes |
| --- | --- |
| Process / PTY | `foundation-*`, `process-pressure-*`, `close-completion-race-*`, `ready-restoration-*`, `runtime-diagnostics-*` |
| Guardian / image | `guardian-*`, `helper-image-*`, `bundled-helper-*`, `candidate6-spawn-*`, `final-census-*` |
| Projection / parking | `loop3-*`, `loop4-*`, `projection-*`, `ordered-transfer-*`, `checkpoint-*`, `parser-control-*`, `control-admission-*` |
| Scrollback / terminal | `terminal-*`, `scrollback-*`, `page-*`, `bitmap-*`, `native-corrections-*`, `resumed-*` |
| Client / interactive | `interactive-*`, `workspace-sdk-*`, `event-stream-*`, `foreground-*` |
| Performance / load | `load-*`, `candidate5-*`, `capacity-*`, `idle-control-*`, `reader-memory-*` |
| Coverage / native | `coverage-*`, `native-boundary-*`, `ci-71c1d6f-*` |
| CI / release | `ci-portability-*`, `ci-workspace-*`, `independent-acceptance-*`, `release-*` |

Filenames are unchanged from when they lived under `docs/reviews/` so evidence
READMEs can be retargeted with a path prefix only.
