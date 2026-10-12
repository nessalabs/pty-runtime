# Native and materializer fault seams: independent DDD/ownership review

Scoped PASS; no new P1/P2 ownership/dependency-direction finding. The user's
explicit test-only fault authorization supersedes the earlier no-new-seams scope.
Reviewed the final five native files and three materializer files against their
actual ownership contracts, plus retained focused execution. No build, test,
measured process or source edit was performed. All five native files match
`work/terminal-native-faults/source-sha256.json`. The final native log records
nine passing tests; existing native integration suites retain their passing
results. `materializer-faults-final02.log` records all ten materializer tests;
final01's nine-test filtered run is not substituted for that evidence.

## Native ABI response ownership

The native seam is private to the infrastructure adapter and cfg(test). Each hook
runs after the real FFI call, receiving initialized outputs owned by that adapter.
No domain/application interface, native ABI, ordinary-build field, feature switch
or configuration control is added. Scope guards are thread-local, non-Send and
non-Sync, reject nesting, allow exactly one consumption and clear during unwind.
The saved tests exercise both thread isolation and cancellation of an armed scope.
Terminal operations do not fork; unlike the materializer, this seam does not
require a post-fork TLS contract.

The tests distinguish read/serialization failures from mutations. Malformed
reads yield no partial public view and leave the underlying owner readable;
checkpoint failure discards the result and preserves the source; failed restore
metadata yields no escaped terminal while keeping the checkpoint reusable.
Mutation errors invalidate the affected owner so it cannot expose a falsely
coherent Rust/native state after the real call may already have changed it.
Restored-owner history failure leaves the independent source usable. Unsupported
compression remains recoverable. Consumption assertions prevent a generic error
from masquerading as the selected injected response.

Invalid color/underline/codepoint values are deliberately injected at the ABI
response boundary. They now test the adapter's defensive contract under a
simulated native violation. They do not prove that ordinary malformed VT makes
the pinned native engine produce those values, or that an actual native OS/error
condition occurred. Real-first calls also mean mutation failures model an error
after possible native effects, not rollback of the native operation. These are
appropriate stated scopes for the existing fail-closed ownership policy.

## Materializer syscall and fork boundary

The syscall macro selects fault handling only under cfg(test); the non-test arm
is the original libc expression. Fault parameters and module disappear from
ordinary signatures. Production retry/order/error-policy logic remains in the
materializer rather than moving into a general mocking layer.

A Copy scalar plan and retained report fd are created before fork and passed on
the stack. Child consumption performs scalar matching/decrement, one libc write
and errno assignment; it does not access TLS, mutexes, allocation, callback
objects or Rust destructors. The reporting socket owner remains in the parent;
child `_exit` closes the copied descriptor. Tests require the exact report bytes,
so a missed/erroring report does not silently pass as fault consumption. The
small bounded one/two-failure plans do not create a sustained reporter queue.

Fork is only replaced by a failure in the reviewed tests. Wait injection is only
EINTR, then real waitpid observes the actual owned child. No successful PID,
wait status or child death is fabricated; the existing unreaped-child signalling
precondition remains intact. Open/write/finalization failures use the real owned
path and real child, assert inode cleanup, then perform a successful independent
retry. Injected close failure skips close at that call; child exit still owns final
fd release. This is a response-failure test, not evidence of every platform's
ambiguous close-error semantics.

Pre-fork failures verify no file publication and unchanged calling-thread mask.
Interrupted open/write/wait/restore retries verify exact bytes and final mode.
The persistent mask-restore case correctly asserts both failed restore attempts,
Io and removed output, then observes SIGTERM still blocked before dropping the
test's real restoration guard. Only that guard restores the original mask. The
result must not be described as production restoring signal state despite a
persistently failing pthread_sigmask; it establishes explicit error/reporting and
image nonpublication under that condition.

The final fixtures retain previously passing public materializer cases and add
no private-poison manipulation or weaker expected outcomes. They exercise
infrastructure failure translation and resource ownership without inverting the
domain/application dependency direction. Fresh complete coverage, ordinary-build
verification, independent quality/adversarial decisions and mechanical/performance
gates remain separate. These focused passes alone do not establish 100% coverage.
