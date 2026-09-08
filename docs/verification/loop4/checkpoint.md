# Reviewed implementation checkpoint

This checkpoint preserves loop 4 implementation and evidence on private main.
It is not full release acceptance. The user deferred the known native checkpoint
crash investigation; its failing evidence and open requirement remain retained.

The source includes the packaged Guardian process adapter, ordered continuation,
optional event-stream forwarding, READY/history progress, aggregate diagnostics,
checkpoint cleanup, and release qualification harnesses. Review records include
`../../reviews/loop4-ddd-consolidated.md`,
`../../reviews/loop4-organization-consolidated.md`,
`../../reviews/guardian-correctness.md`, and
`../../reviews/independent-acceptance-tests.md`.
Later scoped fixes and tests have independent follow-up records, including
`../../reviews/projection-fault-architecture-followup.md`,
`../../reviews/load-harness-independent.md`, and
`../../reviews/bitmap-capacity-integration-review.md`.

The canonical source inventory exactly matches the 281-file candidate 2 inventory
that passed the mandatory gate on macOS arm64 and Linux x86_64, with unchanged
source throughout both runs. Gate metadata and logs are adjacent. Five full
attached-load trials also passed; their methodology is receiving an additional
CLI review, and other load modes are still running. Earlier full lifecycle,
attach and race counts are scoped to their recorded first-candidate build.

The requirement-by-requirement audit is
`../release-gap-audit-2026-09-08-followup.md`. Its cutoff predates the second
Linux gate and corrected five-trial load result; those later results are recorded
in the adjacent README and release directory. No unfinished coverage, memory,
native correctness, platform workload or twelve-hour soak requirement is waived
by this source checkpoint. Final CI and release qualification remain separate.
