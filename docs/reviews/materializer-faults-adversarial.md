# Materializer syscall faults: independent adversarial review

Initial review was blocked by P2-1 below. **P2-1 is resolved on independent re-review; no unresolved P1/P2 remains in this scope.** This review is read-only and covers `crates/infrastructure/src/process/image_materialize.rs`, `crates/infrastructure/tests/fixtures/process_image_syscall_fault.rs`, and `crates/infrastructure/tests/fixtures/process_image_materialize.rs`. No builds, test runs, or source edits were performed by this reviewer.

## P2-1: fault receipt EOF depends on unrelated forked children

At `process_image_syscall_fault.rs:73–76`, the owned materializer has returned, but the fixture drops only its parent copy of the report socket before calling `read_to_end`. A parallel test may fork while that descriptor is open, retaining another copy until exec or exit. The existing `process_image_fork.rs` deliberately holds an unrelated child between fork and exec. Consequently an otherwise correct materializer can wait for an unrelated child's lifetime and fail the five-second read timeout. Reaping the materializer does not close inherited duplicates in other children, and CLOEXEC does not help before exec.

The narrow repair is to shut down the report endpoint's write direction after `write_inner` returns and has settled its owned child, then drop it and drain the buffered markers. Socket shutdown affects inherited copies of the same endpoint, so receipt completion no longer waits on unrelated child exit. Buffered markers still establish the exact number of injected failures. Apply the fix, run affected checks, and obtain re-review; the current green run does not disprove this race.

## Other reviewed properties

The fault plan is a scalar Copy value prepared before fork and passed by value. The child hook uses comparisons, bounded scalar decrement, one libc write, and the platform errno slot. It introduces no heap allocation, TLS-based control state, lock, Rust callback, or destructor in the child. Optional test parameters and the syscall macro's interception branches are absent from ordinary builds; the ordinary syscall expressions remain the same.

The tests exercise real staging writes, permissions, exact bytes, mask membership, cleanup and subsequent successful retries. Injected EINTR resumes the actual open/write/wait/mask operation. Injected write zero/error, chmod failure, and close failure remove the staging path; a skipped close is ultimately closed by the actual child exit. Pre-fork failures return before reading uninitialized signal-set outputs. Each armed error must emit exactly the expected real-fd receipt; no ownership-loss wait result or fabricated wait status is injected.

The two persistent mask-restore failures deliberately leave the calling thread blocked. The test observes that state and only then drops its own real signal-mask guard and verifies restoration. This does not prove production restoration after an unrecoverable OS failure. It also does not establish every allocator/syscall/ownership-loss error branch or 100% coverage.

## Evidence binding before repair

`work/coverage-closure/materializer-faults-final02.log` records all ten materializer tests passing. `final01` records only nine and omits the persistent-mask case, so it is not the final evidence for the current test population. The earlier zero-selected-test attempt is retained and is not treated as validation.

SHA256 values read during this review:

- Materializer implementation: `7749a7a2155710d4d9cfa95753fc3300f425129fc82d5c6db5508a36d5277ddd`.
- Fault fixture: `46b436cb0dcd922ae2a1a009755002b0c52984bcbcab03db5e985574381241d8`.
- Behavior tests: `83378b33489ac46a5b9b85c12ae18268e5201f97b4769ea964589ab7e774637e`.
- Ten-test final02 log: `d63db9cb38eb43fd7019d224adb0ffd4903c6a0b02e2149e3d9228258b0271f6`.

## Independent re-review of P2-1 repair

The author extracted receipt collection and first ran the new real-fork regression against the old drop-only implementation. `work/coverage-closure/receipt-fork-failing-before.log` records the exact failure: receipt `read_to_end` returns `WouldBlock` after one second while the unrelated child retains a copy of the report endpoint. This is an executed behavioral failure, not only a hypothetical descriptor argument.

The repaired collector calls `shutdown(Write)` after `write_inner` returns, then drops the report endpoint and drains all buffered receipt bytes. Production materialization and existing assertions are unchanged. The regression creates a real unrelated child with inherited socket copies, collects exactly `F`, and checks `waitpid(child, WNOHANG) == 0` before releasing it. Therefore success cannot be explained by that child exiting and closing the duplicate first. The child branch uses only bounded stack storage, raw poll/read, and `_exit`. Its parent guard releases it on success or unwind and waits on its retained direct-child PID, retrying EINTR; it issues no cached numeric group signal or post-reap kill. The child's ten-second poll limit also bounds a failed release.

`work/coverage-closure/materializer-receipt-fix-green.log` records all 42 infrastructure unit tests passing, including the new receipt regression, all ten materializer tests, the existing concurrent-fork constructor test, and all nine native terminal fault tests. The separately retained Python before/after receipt experiment is supporting evidence; closure relies on the actual Rust regression and affected suite. Fresh canonical coverage and the complete quality gate remain separate requirements.

The implementation and existing behavior-test hashes remain unchanged from the binding above. The repaired fault fixture is SHA256 `4111e4835a2999fa50e937d37cebbfae135c55929a9ed6d094be4406047683b6`; the failing Rust regression log is `c959c1eaa497d9c41a13f5c65779712b935ebeca1198b6c30758afd8d0abcb4f`; the 42-test passing log is `e4fbf048f18ab34a698c1fc1cbfa23eb45209b8c7fb7ad0f491f305a593f911c`. All were independently read and hashed during re-review.
