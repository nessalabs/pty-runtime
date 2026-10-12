# PR 26 malformed app fixture: independent adversarial review

Reviewer: code_quality_gate. Scoped PASS; no unresolved P1/P2. Reviewed only the root-authored correction to `malformed_utf8_fields_follow_their_distinct_protocol_rules`, not my independently authored PR 25 acceptance test bodies.

Revision 0.4 requires a pair containing a byte outside the general value alphabet to be skipped. The existing fixture supplies `app=earlier:app=\xff`, so the correct stored app is `earlier`; the root record's resolved app is also `earlier`. The old None/None expectation encoded the defect fixed in PR 25. Updating both exact expectations corrects the oracle; it is not an assertion waiver or a production behavior change.

Inspected the complete diff. The malformed-ID unchanged-snapshot assertion, record count of one, blocked state, and question kind are retained. The appended valid `msg=b2s=` pair after the malformed app, together with its exact decoded `ok` assertion, strengthens the fixture by proving parsing continues after the rejected pair. Both raw and resolved app remain asserted exactly.

The retained local RED log reproduces the hosted failure: four pass, one fails at actual Some("earlier") versus stale None. The GREEN log passes all five coverage fixtures and 19 acceptance tests. No tests, builds, or source edits were performed by this review during the freeze.

| Source/evidence | SHA-256 |
| --- | --- |
| `crates/domain/tests/program_status_coverage.rs` | `52b45ccbf230b743a20467227a84c95364a6022cac3c0d949931a97844370a88` |
| `work/pr25-blockers/pr26-stale-oracle-red.log` | `6523f6b72e9f0ffa92f09b12b9a88375f10595c5097397ddb9e96231142371d8` |
| `work/pr25-blockers/pr26-stale-oracle-green.log` | `f542b8be66008d0665eeb86d03bbba08f7390a0f137b20242c2ac2c84a8ed5e5` |

This narrow review does not establish the full mechanical gate, performance acceptance, hosted checks, or the strict 100% coverage requirement. Those gates remain separate; the fixture correction must not be presented as clearing any other readiness blocker.
