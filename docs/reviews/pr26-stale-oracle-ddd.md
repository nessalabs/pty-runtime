# PR 26 stale app oracle: independent DDD re-review

Scoped PASS; no P1/P2 finding. Reviewed the sole current source diff,
`crates/domain/tests/program_status_coverage.rs`, at HEAD
`6bb33b4e20bd55bf38926008fdc76940af158e9d`. Reviewed file SHA-256:
`52b45ccbf230b743a20467227a84c95364a6022cac3c0d949931a97844370a88`.
No build, test or source edit was performed; only this record was written during
the coordinator's source freeze.

The correction aligns the test with the already independently reviewed PR 25
grammar boundary: a malformed UTF-8 app pair is skipped before duplicate-key
replacement, so the earlier valid app and its resolved value remain `earlier`.
It does not change production parsing or the distinct rule for a syntactically
valid but semantically invalid app. Independent acceptance tests already cover
that distinction across both terminators, every chunk split and hard byte limits.

The test retains record count, state and kind assertions, plus the preceding
malformed-id whole-report immutability assertion. Adding a valid message after
the malformed app and requiring decoded `ok` also proves parsing continues past
the skipped pair. Exact value assertions replace the stale exact absence
assertions; no assertion is removed in favor of a weaker generic success check.
The report parser still owns grammar, while the test observes public store
results. No dependency direction or runtime ownership changes.

Retained red evidence has four tests passing and this stale oracle failing with
actual Some("earlier"). Green evidence records all five coverage-oriented tests
and all 19 independent acceptance tests passing. These are focused test results;
the running frozen full gate, strict coverage thresholds and final acceptance
remain separate requirements.
