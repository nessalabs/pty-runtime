# Candidate 3 capacity and resource trials

Completed five trials for each of nine cases (45 total) from frozen source 5373d8e. All five raw-capacity trials and all 35 idle/resource trials passed execution/correctness accounting. All measured raw latency and idle CPU targets passed. The five projected-capacity trials failed during warmup at resize admission; latency targets are unmeasured, not passing. The driver exit status is 1 and the summary correctly reports all_trials_passed=false.

This source predates independent control admission and actual reader scratch gauges. Candidate4 addresses the admission failure and has separate full correctness/latency evidence. These older resource records provide a comparison, not final-source proof or a demonstrated 4 KiB control-memory bound. The macOS host also ran scoped tests/builds and candidate4 load work during this run; the data is not a quiet-host comparative capacity baseline.

Raw per-trial source and binary identity, configuration, byte/gap accounting, p99 measurements, process census and requested Rust allocations are retained. Five projected failure files were previously copied separately under parser-control-admission/frozen3-projected-failures; this directory now preserves the entire completed run and summary.
