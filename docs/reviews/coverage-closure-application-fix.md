# Coverage closure application fixture correction

Author correction for the first P2 in `coverage-closure-ddd.md`, 2026-10-11. Independent re-review is required; this record does not resolve the finding by itself.

Removed the direct sequence/spawn-counter assignments, watcher-counter assignment and all four new tests that manufactured internal mutex poisoning. Removed `runtime/lifecycle_tests.rs` and its module declaration entirely. Defensive overflow and poison-recovery branches remain honest coverage gaps where there is no established public path to induce them.

Observer admission now uses the public Runtime, Session and resource snapshot: a local observer limit rejects an additional wait without retaining a shared reservation, dropping the first observer permits a replacement wait, and dropping that wait releases the reservation. The fixture uses the existing external repository/process ports.

Shutdown admission/guard release now uses public Runtime calls and an external backend whose spawn blocks on a channel. A backend shutdown notification establishes that shutdown has begun before the test tries and observes a refused new spawn. Releasing the already admitted spawn lets it return a process; the test observes cancellation on that process and successful shutdown completion. The fixture changes no production visibility or state and has bounded channel waits.

Validation: `cargo test -p pty-runtime-application` passed 150 tests; `cargo clippy -p pty-runtime-application --all-targets -- -D warnings` passed. Raw evidence is `work/coverage-acceptance-followup/review-fix-tests.log` and `review-fix-clippy.log`. No production behavior changed. Full gate, coverage measurement and independent re-review remain owned by the coordinating task.

The preserved raw-session `program_status()` assertion in `runtime/owner_boundary_tests.rs` is stage-2-specific and must be separated if the general coverage tests are later moved to a base branch.
