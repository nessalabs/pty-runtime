# PR 25 benchmark cancellation: independent adversarial re-review

Reviewer: code_quality_gate. Scope: benchmark runner source and independently authored benchmark tests only. Result: scoped PASS, no unresolved P1/P2 in this change. I authored the separate parser regressions and do not approve those tests in this review.

## Findings and closure

The original runner placed Cargo and its benchmark in a new session but handled only timeout cleanup. A real SIGINT could propagate KeyboardInterrupt without terminating that group. The retained initial failure is `work/pr25-blockers/interrupt-red.log`.

The initial repair had two further P2 failures: invoking `communicate` after timeout cleanup had already closed its pipes could replace the interrupt with ValueError and abandon reaping; `BaseException.add_note` was unavailable on the supported Python 3.9 runtime. Failing evidence remains in `interrupt-closed-pipe-red.log`, `interrupt-cleanup-error-red.log`, and the durable regression run `interrupt-edge-red-final.log` under `work/pr25-blockers`.

Re-reviewed final code terminates the unreaped owned process group, closes both parent pipe endpoints directly, and waits for the direct child using one ten-second cleanup deadline. It never re-enters communicate during interrupt cleanup. It retries individual cleanup operations after another KeyboardInterrupt while retaining the original interrupt object. Once the deadline expires, cleanup failures remain attached as a tuple on that original exception; no Python 3.11-only exception API is used. A child already reaped is never used for another cached numeric group signal. The successful and timeout result contracts remain unchanged.

## Independent acceptance evidence

Read the actual final source and all seven durable tests. `interrupt-edge-green.log` records seven passes in 1.369 seconds. The real-process fixture verifies distinct process-group ownership, delivers SIGINT to the runner, observes interrupt propagation, and checks that launcher and descendant are no longer running. Its unwind fallback instructs the still-live fixture through its retained socket to terminate its own group rather than signalling a stale descendant number.

The other real-process tests preserve successful stdout/stderr/exit status and terminate a timed-out launcher plus descendant. Four orchestration tests exercise interruption after timeout pipe closure, repeated interrupts with decreasing timeouts and the same original exception, no group signal after reaping, and cleanup failure retention without losing the original interrupt. I proposed these boundary checks independently; the root agent authored them. No build or test was launched by this review during the source freeze.

## Source and evidence binding

| File | SHA-256 |
| --- | --- |
| `scripts/program_status_performance.py` | `7fff923f914678b39c303755714c8f236f1f7c2ddd5c2e965b430403c77892a8` |
| `scripts/tests/test_program_status_performance.py` | `7291b8f2999cdd2524228aed164ce7c72df9f0f8954d6863b3f043283954cfcc` |
| `work/pr25-blockers/interrupt-edge-green.log` | `8faeaab7fe0f8043f608ddb15f5cf75efa3557ff957a6fbf175b3ba83e5a4f26` |
| `work/pr25-blockers/interrupt-edge-red-final.log` | `67790d8a80f0c667a72f76e6bb9f398765754ae98a671e13cdf06b660dbb8f0a` |

## Limits

This review does not claim parser acceptance, 100% coverage, full mechanical/performance acceptance, Linux execution, or complete release qualification. Process checks permit a dead zombie descendant; they prove absence of running descendants, not direct ownership/reaping of grandchildren. Cleanup remains bounded and reports failure if the operating system refuses termination/reaping or cancellation prevents completion through the deadline. The fault tests verify orchestration at precise exception boundaries; they do not simulate every possible signal instruction boundary. The owning task must retain the full gate and coverage results separately.
