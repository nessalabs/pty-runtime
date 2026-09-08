# Close-completion race and fixture diagnostics: organization review

No P1/P2 organization, layering, cohesion or ownership finding in the production close fix and failure-context fixture additions. This is an independent review of those changes, not a self-review of the regression tests authored by this reviewer. Other reviewers own their behavioral/test review. No local build, test or workload was run.

`ProjectionCoordinator::close` remains the application admission boundary. The fix consults the existing durable `close_outcome()` only when waking reports an error. If closure has already completed, it returns the original ticket/wait and preserves the worker's recorded success or storage-cleanup failure. If closure remains unfinished, the wake failure is still returned. Ticket-admission Capacity is also retained through the unchanged final `pair.map`; no request-budget increase or admission bypass is introduced. `close_outcome()` derives completion under the existing core mutex, with no OS, native, executor or serialization dependency introduced into application logic.

The final outcome remains owned by cleanup. Worker cleanup records Closed and the durable cleanup error, completes its waiters, then releases services and the scheduler handle. The caller no longer mistakes that legitimate handle release for failed cleanup. The fix does not synthesize success from a missing handle alone, erase a cleanup failure, add a retry loop, or change the ordering of publication/rejected operations/notifications. Lock ownership stays narrow; wake and provider notification remain outside the core lock.

The release fixture's `diagnose` helper observes errors and returns the same `Result<T,E>`. Context consists of static phase, turn, deterministic seed, session lifetime and public status values; it does not read native internals or print PTY bytes/commands/input. `finish_observed` preserves cancellation, explicit exit request, wait admission, timeout, completion and forget as separately named boundaries. Its original supervision/admission/exit/drain assertions remain intact. General fixture callers route through `finish(..., None)` without new logging; the soak supplies a turn for its transient operations.

`observe_projection` preserves the original deadline and RuntimeError conversion for periodic view/checkpoint/resize. The projected resize still requires both OS and model outcomes to succeed. Workload rate, turn frequency, populations, cold cycles and final byte/resource assertions are unchanged by this diff. Diagnostics are best-effort synchronous stderr/public-status observation after failure; they are not a new guaranteed wall-clock-bounded telemetry channel. They cover the newly wrapped periodic/transient boundaries and do not claim every possible soak failure now has context.

All reviewed files remain below 350 nonblank lines. Exact final identities and counts are retained in `docs/verification/projection-close-wake-race/organization-source.json` and below. The coordinating agent owns Linux RED/GREEN and full checks; this report does not turn an interrupted or failed soak into a successful stability claim, and native-crash work remains outside scope.

| File | Nonblank lines | SHA-256 |
|---|---:|---|
| `crates/application/src/projection/admission.rs` | 227 | `c13ae04a2d47d6524bd52ac613ddebdf4ee81800e0c61cdc8969fdb7fd674c8a` |
| `crates/application/src/projection/teardown.rs` | 140 | `c27f771fcec8f751e35aaea5de0665f98844a13050a300783264b53056bcac22` |
| `crates/application/src/projection/worker.rs` | 226 | `0c196c99d94ad93c7487e23d256894567389215815450eb77e6b7d726c14e5ef` |
| `examples/release_support/mod.rs` | 202 | `b5ed1f5f5230a7fce4a41df12967898d73ac9ed92a8ca10cd166b559c454a866` |
| `examples/release_support/soak.rs` | 263 | `7faea6420df8dbd1b04e35a53341efe9aa5d924bb0a024260357f257a835d128` |
