## Session handoff — where things stand

**main** is `e01c704`. PRs #6-#15 are merged.

### Open pull requests

They touch separate files, but the order is **not** free: **#17 must merge
before #18.** Until it does, `docs/verification.md` on `main` still says no
128-producer workload has run, while this branch's notes record `128-active`
passing 5/5 — so merging this one first would put two contradictory accounts
into `main` at once, with the authoritative ledger holding the wrong one. #16
is independent of both. #19 is already merged.

| PR | Branch | State |
| --- | --- | --- |
| #16 | `pr-conventions` | PR template and the `AGENTS.md` rules behind it. |
| #17 | `g1-load-evidence` | Experiment 0005 and its data, plus the `verification.md` update. **`verification.md` is only current on that branch** — on `main`, and therefore on this branch, it still reads as though no 128-session workload has run. |
| #18 | `chunk-batching` | This branch: the batching fix, its tests, and the session notes. |
| #19 | `harness-descriptor-limit` | **Merged** as `b6f6ddd`. Records `ulimit -n` per run; documents what the 128-session cases need. |

### Where the 128-session evidence actually stands

`128-active` **has been run and passes** — 5/5 at `ProjectedOutput` p99
8.0-9.1 ms against a 20 ms target, holding 10.0 MiB/s, with zero histogram
overflow. It needs `ulimit -n 65535`; at the common 1024 default it cannot
start at all, which is a host limit and not a property of the runtime (#19
records the limit, and reverting the patch reproduces the same failure).

The authoritative record for this is Experiment 0005 on #17. Treat the numbers
here as a pointer to it, not as a second source.

### The box

`bx_pvnvgsk9` — Linux x86_64, Ubuntu 24.04, 8 vCPU, kernel 6.8.0-117. Auto-stops
when idle; `box resume bx_pvnvgsk9` brings it back. Repo cloned at
`/home/user/ptyrepo` at `e01c704`, native bootstrap done, release example built.
The batching patch is applied there as a working-tree diff, together with the
fixture path instrumentation (`git stash`/`git stash pop` was used; check
`git status` and the binary's hash before trusting the tree — a stale build
caused one measurement to be re-run).

It is the only quiet host available and the only Linux evidence. Keep it.

### What G1 is actually about

ADR 0004 gate G1 is "process and bytes": real PTY spawn, ordered I/O, bounded
replay, independent observers, cancel/drain and cleanup, **passing concurrent
pressure tests**. Everything except that last clause was already proven. The
clause was unproven because nobody had run the pressure matrix — not because
anything was known to be wrong.

Running it found a real defect: `ProjectedOutput` p99 misses its 20 ms target
because per-chunk scheduling overhead dominates at small chunk sizes. That is
what `chunk-batching` fixes. The fix has five-repeat full-duration evidence on
four cases: `chunk-64` 0.1 ms every trial, `chunk-1` 5.5-6.0 ms, `attached`
0.2-0.3 ms, and **`128-active` 8.0-9.1 ms, passing 5/5**. `128-active` needs
`ulimit -n 65535` to run at all — at the 1024 default 128 sessions do not fit
in the descriptor limit and the case fails before measuring anything, which is
a host limit rather than a property of the runtime. A fifth case,
`capacity-projected`, was also re-run and **still fails** at 255-331 ms.
