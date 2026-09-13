# Outstanding work

What is not done, why it matters, and what "done" looks like. Status of what
*is* done lives in [`../verification.md`](../verification.md); this file is only
the open end.

| | |
| --- | --- |
| [`release-blockers.md`](release-blockers.md) | Must close before calling a release ready. Long-running; needs decisions about hardware and duration. |
| [`code-cleanups.md`](code-cleanups.md) | Small, self-contained, safe to pick up cold. |
| [`declined.md`](declined.md) | Considered, measured, and rejected. **Read before starting anything that looks like these** — the measurement is recorded so the work is not redone. |

## Two conventions worth keeping

**Say what a test proves, not what it is named.** Several entries below exist
because a test passed while proving nothing: assertions that could not fail in
the case they existed to catch. When closing an item, mutation-check it — break
the thing deliberately and confirm the test fails — and confirm the edit
actually applied before trusting the result.

**Do not mark a clause covered by a mock.** `AGENTS.md` is explicit that mocks
verify orchestration only. Cross-layer claims about a real child need a real
child.
