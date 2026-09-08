# Candidate 2 execution results — 2026-09-08

Release qualification remains incomplete. These results supplement, without
rewriting, the earlier release gap audit. The frozen candidate is
`work/release-candidate-2`; each retained workload records its own source and
binary identity. The candidate predates the new capacity producer and Python
reporting changes. Main checkpoint `f9da4e1` contains the matching source, with
platform gate inventories retained under `../loop4/`.

## Completed workloads

- macOS attached: all five full 60-second post-warmup trials passed execution,
  exact byte/query/gap accounting and all five recorded latency targets.
  Evidence: [summary](load-attached-candidate2/summary.json).
- Linux attached: all five full trials passed the same checks.
  Evidence: [summary](linux-candidate2/load-attached-candidate2/summary.json).
- macOS detached, stalled observer and stalled event-stream sink: five full
  trials each passed correctness and all recorded latency targets.
- macOS dominant producer: four full trials passed; trial five failed with
  `Projection(Capacity)` before completion. No final latency result exists for
  the failed trial. The combined 20-trial observer run exited 1 and is failed,
  not accepted on the basis of its 19 successful trials.
  Evidence: [summary](load-observers-candidate2/summary.json) and
  [failed trial](load-observers-candidate2/dominant-5.jsonl).
- Linux completed the full 10,000 lifecycle / 100,000 attachment repetition
  workload and 256 race rounds. Raw evidence is under
  [the Linux candidate directory](linux-candidate2/).

Controlled offered-load results do not establish maximum throughput. Their
latency records are scoped to the measured internal boundaries; fixture RTT
has no substituted internal-dispatch SLA. The development host ran other
verification work during portions of these trials, so these are not a quiet
reviewed-host regression baseline. Missing CPU/process-tree measurements are
not inferred to be zero. The old driver summary labels execution only; target
counts above were read from the individual retained trial records.

## Still running / outstanding

The real 12-hour Linux mixed soak began at 2026-09-08 19:58:13 UTC, using the
frozen candidate and its recorded release build. It was still running at this
report cutoff, without a completion result; neither completion nor memory
plateau is claimed. Expected wall-clock completion is approximately
2026-09-09 07:58 UTC, followed by result and resource-plateau review.

The dominant failure requires diagnosis and qualification. Full remaining
population/rate/chunk/grid/resource workloads, five-repeat capacity baselines,
100% scoped coverage and the previously recorded release gaps remain open.
The separately known native crash remains unresolved and user-deferred; these
ordinary load results do not close it.
