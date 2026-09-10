# Verification

Executed evidence and the proof ledger. Experiments and archive reviews are not
substitutes for ledger rows.

## Ledger

[`requirements.md`](requirements.md) — requirement → code → command → platform →
artifact. Prefer updating this file when closing gaps; do not invent new top-level
narrative docs for each investigation.

## Evidence by feature

Folders below are raw artifacts (logs, metadata, READMEs for a specific run).
Feature intent lives in [`../features/`](../features/README.md).

### Process / PTY ownership

`foundation/`, `loop2/`, `process-pressure-race/`, `close-completion-race/`,
`projection-close-wake-race/`, `foreground-prototype/`

### Guardian / helper image

`guardian/`, `bundled-helper-image/`, `helper-image-fork-contract/`, `image-fork/`,
`candidate6-spawn-fixed/`, `candidate6-spawn-trace/`, `candidate6-spawn-untraced/`,
`spawn-diagnostic6/`

### Projection / parking / checkpoints

`loop3/`, `loop4/`, `parser-control-admission/`, `projection-io-contract-tests/`,
`projection-io-faults/`, `checkpoint-crash-cleanup.md`,
`control-admission-ddd-review/`

### Scrollback / history

`scrollback/`, `page-admission/`, `page-capacity/`, `continuation-c1/`,
`decode-budget/`, `mouse-modes/`, `packed-pages/`, `bitmap-capacity-*`

### Client / demo

`interactive/`, `interactive-review/`, `workspace-sdk-sketch/`

### Performance / load

`load-capacity/`, `load-capacity-independent/`, `load-capacity-organization/`,
`load-methodology/`, `load-*`, `reader-memory-gauges/`,
`reader-memory-gauges-independent/`, `projected-capacity-methodology/`,
`release/` (candidate load runs)

### Coverage / native

`native-coverage/`, `native-instrumentation/`, `native-boundary-independent/`,
`coverage-*`, `public-boundary-contracts-71c1d6f/`

### CI / release

`ci-*`, `ci-portability/`, `release/`, `resumed/`, `release-gap-audit-2026-09-08/`,
`remaining-acceptance-71c1d6f/`, `review-remediation/`

Milestone narratives that used to live at the docs root (`loop2`, `loop3`,
foundation, gap audits) are archived under
[`../archive/milestones/`](../archive/milestones/).

## Rules

1. Do not treat a green local experiment as a discharged ledger row.
2. Prefer appending evidence under an existing feature folder over a new top-level
   name when the investigation is the same concern.
3. Link specialist reviews from evidence READMEs to
   [`../archive/reviews/`](../archive/reviews/) (historical) or record findings in
   the ledger when closing a milestone.
