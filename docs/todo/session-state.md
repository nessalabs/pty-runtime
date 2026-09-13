## Session handoff — where things stand

**main** is `e01c704`. PRs #6-#15 are merged.

### Branches not yet pushed

| Branch | Commit | State |
| --- | --- | --- |
| `g1-load-evidence` | `24a6f60` | **Ready.** Experiment 0005 artifact + verification.md update. Gate not yet run on it since the split. |
| `chunk-batching` | (see log) | **Measured.** Batching fix + 2 tests + path instrumentation. Four cases at five repeats run on `bx_pvnvgsk9` 2026-09-13: 15/20 pass, the five `128-active` failures are the host's `ulimit -n 1024`, not the patch. See `docs/todo/code-cleanups.md` first item. |

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
what `chunk-batching` fixes. The fix now has five-repeat full-duration evidence
on three cases (`chunk-64` 0.1 ms every trial, `chunk-1` 5.5-6.0 ms, `attached`
0.2-0.3 ms). The fourth case, `128-active`, needs `ulimit -n` raised on the host
before it can run at all — 128 sessions do not fit in 1024 descriptors.
