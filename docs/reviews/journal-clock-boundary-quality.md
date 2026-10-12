# Journal delegation and clock failure quality review

Initial verdict: journal-only cleanup passes source review; clock test has one
P2 oracle-specificity finding pending correction and re-review. This reviewer
ran no builds or benchmarks.

Exactly seven journal poison-recovery closures become
`PoisonError::into_inner` method references, with a corresponding standard-library
import. Lock acquisition, returned guard, temporary lifetime and recovery policy
are unchanged. This removes pure delegation without adding an abstraction,
changing dependency direction, or suppressing source coverage.

The new public-port clock test correctly selects the caller thread and releases
its own clock mutex before panic. Source inspection confirms the runtime context
lock and queue admission lock are held at the injected clock call, after a
provisional staging allocation but before output policy admission commits. The
assertions exercise typed Internal/Closed behavior, observer lease rollback,
zero publication and staging/replay reservations, and eventual native teardown.
This is existing behavior: saved evidence shows the test passed before cleanup.

## P2: prove the intended clock injection occurred

The initial assertion accepts any unwind from `events.output`, without checking
that the clock consumed its armed one-shot fault. An unrelated earlier panic
while holding the context lock could satisfy the same Internal/Closed and
zero-publication assertions, leaving the clock armed. This would misidentify
the exercised boundary. Require a deterministic observation immediately after
catching the unwind that the clock's armed caller was consumed. The fixture
already exposes that observation under its own mutex, which is released before
panic. No production change or additional fixture is required.

The author was asked to add the observation, rerun the affected test and return
for re-review. Saved final-suite evidence before this requested correction
reports 152 application tests and eight integration tests passing; that does
not itself close the oracle-specificity finding. Full mechanical, coverage,
performance and separate adversarial gates remain distinct requirements.

## P2 correction and independent re-review

The test now immediately asserts that `clock.0.lock().unwrap().is_none()` after
catching the unwind. Only the selected caller's clock read consumes this arm;
background clock reads leave it intact. The clock releases its own mutex before
panicking, so the observation is deterministic and does not rely on poisoned
fixture state. An unrelated earlier callback panic can no longer satisfy the
test. No production behavior or additional fixture was changed.

Read `work/journal-clock-boundary/clock-quality-rereview.log`: the focused
`cargo test --all-features --test projection_correctness clock_panic -- --nocapture`
rerun passes its one selected test, with the expected injected CallerClock
panic at line 18. Independently inspected the corrected source and this retained
evidence. **P2 closed; final scoped quality verdict PASS**, with no unresolved
P1/P2 findings. Previous initial finding and earlier evidence remain above.
The full mechanical/coverage/performance gate is still separate. The reviewer
performed no builds or benchmarks.

Reviewed final SHA-256 values:

- `crates/application/src/projection/journal.rs`: `38744a7352a0e2dbad4fb70fc156690dc56c524db4eefb359af19af8811e9443`
- `tests/projection_correctness/clock_failure.rs`: `2437bd2ae300b8e776d2dc8ff50b886b66c79df4f2e69b6f875d42f718d5b4b0`
- `tests/projection_correctness.rs`: `3c8a375663f711066840af461fdd869b24554137cc85b55656c5c21805f0582a`

## Public waits already pending at the clock fault

Reviewed the added
`existing_output_and_completion_waits_report_internal_error_after_clock_failure`
test independently. Both publicly admitted completion/output futures are
explicitly polled Pending before the same caller-specific clock fault. The
consumed-arm assertion proves the intended injection. Subsequent explicit polls
must produce Ready(Internal), the attachment cursor remains unchanged, and
synchronous try_next also returns Internal before shutdown closes residency.
This exercises the existing future error paths rather than injecting private
lock state. It does not prove autonomous wake delivery after the callback panic:
a noop waker and explicit polling intentionally limit the claim.

The current matrix's saved `full-current-continuous01/workspace-all-features.log`
records the added test passing with all seven projection_correctness tests.
This is actual execution evidence for this test; the complete matrix remains
in flight and is not marked passed here. No new P1/P2 finding was found;
**scoped test-quality PASS**. No additional build was launched by the reviewer.

Updated clock fixture SHA-256: `395d1f698f858ecc42681e3a3bac0a383962a595d55833ac07f59850043541a9`.
