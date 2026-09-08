# Full release workloads: first frozen-source runs

## Candidate 2 follow-up

`full-build-candidate2` records a fresh release build after the reviewed fixture
readiness and collector-diagnostic corrections. All five full attached-load
trials passed in `load-attached-candidate2`; the earlier failed run below remains
unchanged. The candidate's macOS and Linux gate inventories match exactly.
Passing this one case does not complete the load matrix. Independent methodology
review and the remaining workload cases are in progress.

## First candidate

These results are partial qualification on macOS arm64, using the frozen source
and release build recorded in `full-build-1`. Its source inventory matches the
macOS/Linux mechanical-gate source in `../loop4`; later working-tree changes are
not represented by this binary. All JSONL files retain source and executable
identity. No reduced workload was used for these runs.

| Workload | Result |
| --- | --- |
| 10,000 spawn/exit-or-cancel cycles and 100,000 attach/drop operations | Passed in 65.8 seconds; `repetition-full-1.jsonl` |
| 256 seeded concurrency race rounds | Passed in 3.75 seconds; `races-full-1.jsonl` |
| Attached load, 64 resident / 16 active, 10 MiB/s, five 60-second post-warmup trials | Failed overall: trials 2 and 5 passed; trials 1, 3 and 4 failed; `load-attached-full-1` |

Repetition's baseline and final owner FD counts were both eight. Quiescent
process censuses found only the owner and no zombies; in-process assertions
checked resource reservations and exact byte accounting. Final RSS was 4,210,688
bytes versus 3,375,104 at baseline. The run ended before the 120-second warmup
cutoff and cannot establish a memory plateau.

Both successful load trials passed the measured latency targets, but they cannot
substitute for five successful repeats. Trial 1 ended in a Python assertion;
trials 3 and 4 reported `WouldBlock` from the executable. Root causes are under
investigation. Preserve all failed records when adding corrected runs.

The remaining load cases, full twelve-hour soak, cross-platform full workloads,
terminal corpus completion and strict coverage target remain outstanding. The
Linux machine stopped before the later release build could be recovered; its
earlier completed mechanical-gate evidence was already copied into the repository.
