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
