## Session handoff — where things stand

**main** is `e01c704`. PRs #6-#15 are merged.

### Branches not yet pushed

| Branch | Commit | State |
| --- | --- | --- |
| `g1-load-evidence` | `24a6f60` | **Ready.** Experiment 0005 artifact + verification.md update. Gate not yet run on it since the split. |
| `chunk-batching` | `56c013f` | **Incomplete.** The batching fix + 2 tests. See `docs/todo/code-cleanups.md` first item. |

### The box

`bx_pvnvgsk9` — Linux x86_64, Ubuntu 24.04, 8 vCPU, kernel 6.8.0-117. Auto-stops
when idle; `box resume bx_pvnvgsk9` brings it back. Repo cloned at
`/home/user/ptyrepo` at `e01c704`, native bootstrap done, release example built.
The batching patch is currently applied there (`git stash`/`git stash pop` was
used; check `git status` before trusting the tree).

It is the only quiet host available and the only Linux evidence. Keep it.

### What G1 is actually about

ADR 0004 gate G1 is "process and bytes": real PTY spawn, ordered I/O, bounded
replay, independent observers, cancel/drain and cleanup, **passing concurrent
pressure tests**. Everything except that last clause was already proven. The
clause was unproven because nobody had run the pressure matrix — not because
anything was known to be wrong.

Running it found a real defect: `ProjectedOutput` p99 misses its 20 ms target
because per-chunk scheduling overhead dominates at small chunk sizes. That is
what `chunk-batching` fixes, and the fix is measured but not yet trustworthy.
