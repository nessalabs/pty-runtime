# Target measurement state correction

Independent review found P2: a present target with unavailable CPU or p99 was
recorded as false, so its summary reported a measured failure instead of missing
evidence. The actual unavailable-owner-CPU subprocess fixture now expects null;
it failed against the prior implementation before the correction.

Target evaluation now has three states. A missing observation or unavailable
samples produces null when no actual failure is established. A measured threshold
miss or explicit operation failure produces false, including when other evidence
is incomplete. A complete in-range measurement with no failures produces true.
Raw latency failure/unavailable counts and measurement completeness are retained.

Per-trial latency results use the same tri-state aggregation, replacing all()
which would convert null into false. Summaries separately count failed and
unmeasured trials, with incomplete_trials preserving incomplete evidence even
when a genuine failure determines the verdict. Missing latency boundary lists
also preserve that distinction. Correctness execution stays separate.

All 18 load tests pass with ResourceWarnings treated as errors. Tests cover the
actual null-CPU fixture, observed target misses, null p99, partial unavailable
measurements, and explicit failure dominance with a missing-boundary record.
Red/green logs and source hashes are under
`docs/verification/release/load-target-tristate`. Independent re-review passed
with all 18 load tests and five pure validator tests rerun; see
`docs/archive/reviews/load-python-contract-review.md`. No frozen candidate or
runtime/native source changed.
