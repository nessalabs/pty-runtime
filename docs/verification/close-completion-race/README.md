# Close completion versus wake race

An already scheduled projection worker can observe Closing, finish cleanup and release its handle before close() calls wake(). Previously close() returned Worker even when its durable cleanup result was already available. The root fix consults close_outcome only after wake failure: an unfinished close still returns the scheduling error, while a finished close keeps its original completion ticket and actual cleanup outcome.

An independent test author forced this ordering through a one-shot capacity notification and the real application worker/cleanup paths. Before the production fix, Linux executed all three regressions: clean cleanup and failed storage cleanup both failed their public close-result assertion; unfinished missing/rejecting scheduler passed. After the fix all three pass, including zero transient reservations and retained charged unreclaimed storage on real delete failure. All55projection tests and application all-target Clippy also pass with unchanged recorded source.

These runs use a separate Linux diagnostic clone of71c1d6f plus the exact test and fix patches, leaving both frozen release candidates untouched. They cover the production fix and test file hashes recorded in metadata. Later module-declaration sorting is formatting only. Fixture-only soak error-stage diagnostics and the full combined current-source gate are separate evidence.

The failed candidate5 soak reported Projection(Worker) after turn576, just before a projected transient creation/cleanup operation. These deterministic tests prove a real close race discovered during that diagnosis, but the original log did not identify its failing stage, so that soak's root cause is not asserted as established. Error-only fixture diagnostics preserve original errors and identify phase/turn/public status for the next full run. The prior failed soak remains retained; no12-hour pass is claimed.

Independent correctness, organization and DDD reviews are in docs/reviews/close-completion-race-*.md. The caller's durable storage failure and Capacity observation policy remain unchanged; only an obsolete wake failure after finished closure is suppressed.

## Combined checkpoint verification

The macOS mandatory gate, release examples build and 270-second restore smoke all passed with unchanged, identical 294-file source manifests retained in their metadata directories. The smoke completed in 270.55 workload seconds with zero active sessions, zero retained replay, every reported budget released, 39 cleanups and no failed operations or cleanups. It ran alongside the gate and is scoped lifecycle/performance smoke evidence, not a quiet-host baseline or the required 12-hour soak.

All three specialist reviews found no P1/P2 blockers in this change. Linux deterministic RED/GREEN remains distinct from the forthcoming full Linux combined-source gate. Release acceptance remains open for the full soak, outstanding workload cases, coverage and the user-deferred native crash investigation.
