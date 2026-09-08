# Candidate5 failed full soak

The intended12-hour Linux run on71c1d6f ended with Error: Projection(Worker) after about146seconds, exit1 without completion. The original full record is ../candidate5-soak-failed.jsonl. Last completed progress:576turns,590875verified bytes,1768421explicit gap bytes,560parked observations. No native abort/panic was reported. The first operation after that progress includes a projected transient creation and cleanup; Rust-side stage diagnosis is pending, not a proven root cause. This is a failed run, not a superseded or completed soak.

The read-only post-run check confirms all25PIDs sampled at120seconds are absent. The collector did not retain start-time identities and did not sample every later transient; this bounded absence check is not an expanded claim covering unrecorded descendants. No signals or cleanup mutations were performed by the check.
