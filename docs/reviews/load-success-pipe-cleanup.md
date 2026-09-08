# Successful load-trial pipe cleanup

The independent reviewer observed ResourceWarnings after successful trial
fixtures. A focused actual subprocess test confirms the owner exited 0 and all
output was consumed, but trial() returned while its stdin/stdout wrappers were
still open. The failing assertion and original warnings are retained in
`docs/verification/release/load-success-pipes/before.log`.

Canonical load.py now joins the output reader after EOF and confirmed process
exit, then closes the owned pipes through the same OSError-contained helper used
by failure cleanup. The failure path still preserves queued output and reports
exit status before close; BrokenPipe on flushing a failed acknowledgement is
recorded rather than escaping the trial. The frozen candidate is untouched.

The regression verifies exit 0, both wrappers closed, and the final fixture
stderr retained before trial_result. All 15 load tests pass with
`-W error::ResourceWarning`; the existing failure/acknowledgement cases also pass.
Green log and source hashes are in the same evidence directory. Independent
re-review passed in `docs/reviews/load-report-independent.md`; this does not
repeat or qualify the full workload.
