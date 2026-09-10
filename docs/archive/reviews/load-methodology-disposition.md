# Additional CLI review: performance methodology

The user requested an additional CLI agent review. Claude CLI completed a
read-only review of the load harness and the five candidate 2 attached trials.
Its exact request and returned text are preserved in
`../verification/load-methodology`. The deferred checkpoint investigation was
outside its scope. Findings below were checked against source and ADR 0002;
the returned severity labels were not accepted without that check.

| Observation | Disposition |
| --- | --- |
| Fixed offered rate does not establish maximum throughput | Confirmed qualification gap. Existing runs establish keeping up with their configured rate. A separate bounded saturation workload is being implemented; existing controlled cases remain required. |
| Fixture RTT maximum exceeds InputDispatch p99 target | Not an ADR violation: these are different intervals and statistics. ADR 0002 requires fixture RTT separately and baseline comparison; it supplies no absolute RTT ceiling. Preserve reported RTT and qualify baseline comparisons separately. |
| CPU sampling omits transient helpers | Confirmed measurement limitation. Owner CPU deltas remain owner measurements; they do not include every helper's cost. Intervals now explicitly disclose matched, unmatched and unavailable PIDs and that complete process-tree accounting is unavailable. No matched processes means an unavailable category, not measured zero. |
| Summary omits target rollups | Confirmed reporting gap. Separate correctness and latency/idle-CPU target rollups now retain passed, failed and unmeasured counts. Missing owner CPU cannot pass an idle target. |
| Darwin CPU time parsing fails after one hour | Not reproduced. Local macOS documentation and actual long-running process samples use growing minute counts, which the existing parser handles. Regression coverage preserves that format; no speculative parser change was made. |
| Small samples and histogram resolution limit comparisons | Valid limits. Empirical p99 of 58 samples is the largest sample, not a well-supported population-tail estimate. The runtime reports conservative bucket upper bounds, not exact percentiles or floored values. A precise relative regression claim needs adequate sample counts and distinguishable measurement intervals. |
| Degraded census not visible in interval | Confirmed reporting gap. Intervals now propagate unavailable PID inventories and unmatched endpoints; matching endpoints still cannot observe a process born and exited between samples. |

The reporting and CPU-scope changes have focused regression evidence and
independent review in `load-report-independent.md` and
`load-target-rollup-review.md`. They do not retroactively change old JSONL data
or turn old execution summaries into full performance acceptance. The full
matrix, reviewed baseline comparisons, complete resource accounting and soak
remain separate qualification work.
