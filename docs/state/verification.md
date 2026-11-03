---
id: "verification"
title: "Check evidence and release gaps"
kind: "guide"
status: "reference"
summary: "Current source tests, bounded models and review do not replace release qualification."
sources: ["AGENTS.md", "coding_standards.md", "scripts/gate.py", "docs/verification.md", "docs/adr/0004-integration-and-release-qualification.md"]
sourceRevision: "c68837e2b7bf1c9009e1a4587a1a58107ec0425c"
diagramLinks: {}
parent: "pty"
---

# Check exactly what was exercised

Source reviewed: c68837e2b7bf1c9009e1a4587a1a58107ec0425c. Every owner page carries
that checkpoint; it is source-review evidence, not a test result. No production
implementation change is included. The repository's [verification ledger](../verification.md)
retains runtime qualification history and unresolved coverage/platform limits.

## Current checks

The initial full mechanical gate passed on macOS arm64 with isolated Rust 1.99.1.
Its repeated feature-matrix runs report 1,079 Rust test passes, zero failures and
four preexisting ignored test occurrences: the real-clock sixty-second parking
qualification, the scoped performance test (run explicitly later by its release
harness), and two host-policy subprocess fixture occurrences. This is an
aggregate, not 1,079 unique tests. No tests were weakened or newly ignored.
Independent DDD, organization/design and adversarial reviews are complete.
DDD review corrected the demo’s bounded cleanup description and the projected
spawn capability condition; independent re-review closed both. Reader journeys
were answerable, and six-model adversarial review found no material mismatch.
Specialist notes remain outside this tree under repository policy; outcomes and
exact final gate revision are recorded in the PR. No release milestone is claimed.
The mechanical gate includes source/dependency checks, Python harness tests,
format/lint, feature-matrix Rust tests, native contracts, Rustdoc, interactive smoke
and scoped performance workloads. Raw logs remain local work artifacts under the
repository's instructions; no gate/load log dump is committed.

See [execution summary](../../verification/state-models/execution-summary.json)
for the baseline source digest, platform, commands and scoped performance metadata.
The runtime workload is one 4 MiB/session trial at 1 and 16 sessions; parser
workloads have five domain trials each. These are scoped observations, not a
before/after speedup comparison or full release performance qualification.

The Nessa Docs browser smoke rendered all 14 pages and 11 diagrams with zero
errors; see [browser receipt](../../verification/state-models/browser-results.json).
This checks the atlas renderer, not the PTY demo.

## Bounded model verification

Six models select completion, replay admission, safe parking release, transfer
sealing, program-status commit and event publication rules. TLC runs a mathematical
specification, not Rust structs or functions. Each owner explains bounds and
abstractions. Every model needs a current passing baseline, independent adversarial
source review and negative controls that break expected semantic rules; syntax
errors do not count. All six baselines passed their 14 configured invariants; all 14 controls broke
the expected rule. The checker explored 99 aggregate states, largest model 27.
Independent adversarial source/model review is complete.
See [current receipts](../../verification/state-models/results.json) and
[mutations](../../verification/state-models/controls.json). With a built Nessa Docs
checkout and its pinned checker cache ready, reproduce using:

```sh
NESSA_DOCS_ROOT=/absolute/path/to/nessa-docs node scripts/state-docs/verify.mjs --write
```

The runner uses the canonical catalog, checker and baseline receipt writer. It
does not install tools or write mutant receipts as passing page evidence.

## Qualification still separate

The strict 100% coverage target remains unmet in the historical ledger; this atlas
does not relabel it passed. Full release requirements include five 60-second
performance repeats, 10,000 lifecycle cycles, 100,000 attach/detach operations and a
12-hour mixed-load soak, failures/timeouts, latency distributions, resource peaks
and cleanup. These are not run by this documentation work. Existing scoped gate
performance checks do not substitute for them.

No target is newly qualified. Current macOS execution cannot prove Linux behavior;
configured CI and cross-builds cannot prove executed platform coverage. Native
fault isolation, hostile session escape, power loss and production integration
remain outside these finite safety models. The [ADR 0004 table](../adr/0004-integration-and-release-qualification.md#platform-qualification)
controls support claims.
