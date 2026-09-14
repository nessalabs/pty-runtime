<!--
Write for a reader with no context on this work. Plain English first,
numbers second, jargon only where it is unavoidable and explained.
Keep every section. If a section genuinely does not apply, say why.
-->

## What problem does this solve?

<!-- Two or three sentences, no jargon. What was wrong, and who it hurt.
Someone outside the project should understand this. -->

## What does this change?

<!-- What you actually did, in plain terms. If it is a behaviour change,
say what behaves differently now. -->

## Evidence

<!-- Numbers, not adjectives. The two kinds below are not interchangeable, and
a change may need one, the other, or both. Keep the heading of whichever does
not apply and say so. -->

### Correctness

<!-- For a defect fix or a behaviour change. Name the test and show it failing
without the change and passing with it. One deterministic run each way is
complete evidence; repeating it proves nothing further. If the test passes
with the change reverted, it is not testing the change. -->

| Test | Without the change | With the change |
| --- | --- | --- |
|  |  |  |

### Performance

<!-- For anything measured rather than decided: latency, throughput, memory.
Give before AND after, the target being measured against, and how many trials.
A single trial is an observation, not a measurement - label it as one if that
is all you have. -->

| Measure | Before | After | Target | Trials |
| --- | --- | --- | --- | --- |
|  |  |  |  |  |

<!-- `coding_standards.md` requires recorded workload, source identity, platform
and raw results. A number without these cannot be tied to the reviewed code or
repeated by anyone else. Fill one row per distinct configuration: if the before
and after numbers were taken on different hardware, kernels or builds, that is
two rows, and the difference is not attributable to this change alone. -->

| | Workload | Source revision | Platform (OS, kernel, CPU, load) |
| --- | --- | --- | --- |
| Before |  |  |  |
| After |  |  |  |

**How to reproduce:**

```bash
```

**Where the data is:**

## Review gates

<!-- `AGENTS.md` requires a gate run and independent specialist reviews for
every review loop. They are not waivable here: report the outcome of each.
"Pending" is a reportable outcome and so is a failure; silence is not, and
neither is declaring one unnecessary. -->

| Gate | Revision | Outcome |
| --- | --- | --- |
| `python3 scripts/gate.py` |  |  |
| DDD review |  |  |
| Organization / design-pattern review |  |  |
| Adversarial correctness review |  |  |

<!-- Findings and their resolutions belong in `docs/verification.md` when they
bear on a gate. Link the record, or say why none was needed. -->

**Findings recorded in `docs/verification.md`:**

### Release qualification

<!-- Does this PR claim a G3 or G4 milestone, or release qualification? State
yes or no — the answer is required either way. If no, the table below is not
applicable and saying so is the entry.

If yes, ADR 0004 requires each of these, and each is recorded as passed,
failed, or NOT RUN. "Not run" is an honest and common answer; leaving a row
blank is not, because a reader cannot tell it from a forgotten one. Link the
integrated results under `docs/experiments`. -->

**Claims a G3/G4 milestone or release qualification:**

| ADR 0004 requirement | Result | Where recorded |
| --- | --- | --- |
| Five 60-second performance repeats |  |  |
| 10,000 process lifecycle cycles |  |  |
| 100,000 attach/detach operations |  |  |
| 12-hour mixed-load soak |  |  |
| Failures and timeouts |  |  |
| Latency distributions |  |  |
| Resource peaks and cleanup |  |  |

## What this does NOT fix

<!-- Required. Every change has edges. Name the cases still failing, the
platforms not covered, the things assumed rather than tested. If you believe
there are none, say that explicitly. -->

## Corrections

<!-- Did this work overturn anything previously claimed - in a doc, an earlier
PR, or a comment? Say what changed and why. Write "none" if nothing. -->

## Risk and rollback

<!-- What could this break, and how would someone undo it? -->

## Where to look first

<!-- Point the reviewer at the two or three files that matter, and say what
question to ask of each. Saves them reading the diff top to bottom. -->
