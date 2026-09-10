# Candidate6 interrupted remaining matrix

The driver is no longer present at the 2026-09-09 inspection, and its unified session handle is absent. No final driver summary exists. The inventory retains45 completed correctness passes, one harness failure and one incomplete trial. The precise cause of the driver termination is not established. The rate-1MiB second trial ends mid-measurement and is not a pass.

The resources-projected-1 first trial reports a failed periodic process census after its successful closed checkpoint, followed by complete and process exit0. That failure is retained, not relabeled a pass. A separate regression/fix must prevent after-close periodic sampling before a new full five-repeat run of that case.

Eight cases have five completed correctness trials here: idle64; raw resources1/32/128; projected resources32/128; 128-active;128-mixed. Performance targets require their own assessment. Remaining eleven cases, including the two incomplete/failed case groups, will use fresh five-repeat runs with recorded source identity. These historical files are immutable.
