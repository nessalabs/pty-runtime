# Considered and declined

Recorded so the work is not redone. Each was measured, not guessed at. If you
disagree, the measurement is the thing to argue with.

## Moving the native engine driving off the coordinator

**Proposed:** give `apply_command`, `native_call` and
`poll_inflight_operations` an `impl NativeWorkspace` of their own, as was done
for the admitted-work state and the blocking jobs.

**Measured:**

| method | collaborators | workspace fields |
| --- | --- | --- |
| `apply_command` | 5 (`queue`, `journal`, `quotas`, `wiring`, `status`) | 4 |
| `poll_inflight_operations` | 3 | 4 |
| blocking jobs, for contrast | **0** | **0** |

Handing five collaborators to an `impl NativeWorkspace` as a context object
renames `self`; it does not split a responsibility. The blocking jobs came out
cleanly *because* they touched nothing — that is what made them extractable.

`apply_command` mutates native state, domain policy and the journal in an order
that matters. A version returning an outcome for the caller to apply is
conceivable but is a rewrite of the trickiest ordering in the module, and would
need the behavioural correctness review re-run against it.

**Would need:** a different idea, not more of the same one.

## Moving `start_read`, `start_park` and `finish_io` off the coordinator

**Proposed:** finish what `InFlight` and `SourceReaper` started and take the rest
of the blocking-I/O lifecycle off `ProjectionCoordinator` too.

**What did move, and why it could:** the in-flight slot and the submission
primitive became `InFlight` in `inflight.rs`, and the deletion lifecycle —
check out, submit, put back spent or unspent — became
`SourceReaper::start_delete` / `finish_delete`. Both came out cleanly because
neither touches the projection's queue, quotas or native state. The deletion
lifecycle in particular is now driven without a coordinator at all, in
`tests/reaper.rs`.

**Measured**, after those moves:

| method | collaborators | workspace fields |
| --- | --- | --- |
| `finish_io` | 5 (`queue`, `journal`, `quotas`, `wiring`, `config`) | 10 |
| `start_read` | 3 (`queue`, `quotas`, `wiring`) | 3 |
| `start_park` | 3 (`queue`, `quotas`, `wiring`) | 3 |
| `history_progress` | 1 (`queue`) | 3 |
| *`InFlight` / `SourceReaper`, for contrast* | **0** | **0** |

The numbers for the three that stayed are unchanged by this work, which is the
point: what was extractable was extractable *because* it touched nothing, and
removing it did not loosen what is left.

`start_read` and `start_park` each acquire from two runtime-shared quotas, ask
domain policy whether the transition is permitted, drive the native engine, and
submit — in an order where every step can fail and each failure has to put back
exactly what the step before it took. `finish_io` is the same shape in reverse
across four job kinds. Handing any of them an `impl IoWorkspace` holding those
collaborators renames `self`; it does not split a responsibility. This is the
same conclusion as the native engine driving above, reached the same way.

**Would need:** a different idea, not more of the same one. The most promising
is narrowing what the two `start_*` methods take from the queue, so the domain
policy call and the submission stop being interleaved — but that is a change to
`AdmissionQueue`'s interface, not a move of these methods.

## Giving `admission.rs` and `teardown.rs` types of their own

**Proposed:** the "every file owns a type" rule, applied to the last two
`impl ProjectionCoordinator` files after `stream_end.rs` was dissolved and
`completion.rs` was merged into `io.rs`.

**Measured:** there is no state for either type to own.

| file | what it does | who owns the state it touches |
| --- | --- | --- |
| `admission.rs` | the public admission API: reserve quota, build a `Command`, push it | `AdmissionQueue`, `SessionQuotas`, `Journal` |
| `teardown.rs` | the cleanup path: drain the workspace, release the wiring | `NativeWorkspace`, `Wiring`, `AdmissionQueue` |

Both are the coordinator's own surface — one the entry points callers use, the
other the exit path — and every piece of state they touch already belongs to a
type. An `Admitter` or a `Teardown` would hold a back-reference to the
coordinator and nothing else, which is the context-object move declined twice
above under a third name.

The rule earned its place against files that were a *method list* split by
direction: `io.rs` and `completion.rs` held the two halves of one lifecycle, and
`stream_end.rs` held two unrelated methods. Neither of these two is that. Each
names one responsibility and holds the methods for it.

**Revisit if:** either grows a field. A file that needs somewhere to put state
has found its type.

## Testing `commit_park`'s `engine_idle` argument

**Proposed:** cover the case where a commit lands with an empty queue but an
in-flight reply or resize.

**Measured:** unreachable. `serve` only reaches `start_park` after
`poll_inflight_operations` has returned `None`, which requires both the reply and
resize slots to be clear, and nothing can set either between `start_park` and the
`finish_io` that lands the commit. An assertion at the call site never fired
across the whole suite.

Writing a test means constructing a state the worker cannot produce. The
parameter stays as defence against a future path that commits without draining
in-flight native work first; the reasoning is recorded at the parameter itself.

**Revisit if:** a path is ever added that starts a commit without that drain.

## Hard-gating file size

**Proposed** by two automated reviewers on PR #2: restore
`scripts/gate.py`'s fail-closed check on files over 350 nonblank lines.

**Declined** by the repo owner on that PR. The gate reports and continues. The
reviewers' genuine finding — that `coding_standards.md` still described a hard
gate — was real and is fixed; the two now agree.

The same change *widened* the inventory to include `client/`, which the gate had
never looked at, so it reports strictly more than before. Dependency direction,
formatting, clippy `-D warnings` and the test suite all remain fail-closed, and
`scripts/tests/test_sizes.py` pins both halves of that split.
