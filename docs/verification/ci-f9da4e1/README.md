# CI for f9da4e1

The runtime workflow passed macOS, Linux and MSRV checks. The separate PTY
experiment workflow failed on both hosts because its validator treated packed
pool lifecycle records as ordinary memory records and accessed an absent field.
The retained failure is not a native execution verdict. The validator correction
and five regression tests are documented in
[the mixed-record review](../../archive/reviews/experiment-validator-mixed-records.md).
A subsequent successful workflow is required before calling the correction
CI-verified. The exact prior workflow status, revision and job URLs are retained
in the JSON records alongside the raw failing log.
