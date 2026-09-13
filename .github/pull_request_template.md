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
is all you have. Say what varied between runs and what did not. -->

| Measure | Before | After | Target | Trials |
| --- | --- | --- | --- | --- |
|  |  |  |  |  |

**How to reproduce:**

```bash
```

**Where the data is:**

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
