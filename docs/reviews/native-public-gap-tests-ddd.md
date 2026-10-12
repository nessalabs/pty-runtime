# Native/materializer public gaps: independent DDD review

Scoped PASS; no new P1/P2 ownership or dependency-direction finding. Reviewed the
actual three test-file diffs, existing terminal factory/projection and materializer
implementations, and `work/native-public-gaps` source manifest and execution logs.
All three current test hashes independently match the retained manifest. Twelve
terminal tests and six materializer tests pass, including all six additions. No
build, test, measured program or production-source edit was performed by this
reviewer. The coordinator permits docs during the current source freeze.

The terminal tests remain at the existing ITerminal/GhosttyTerminalFactory
boundary. Ordinary VT bytes exercise actual engine-to-domain conversion for the
five underline forms, reset and X10 press-only tracking. Expected domain values
are asserted independently of the converter implementation. Encoding, screen
contents, cursor and already-emitted cells retain their own contracts; the tests
do not collapse tracking and encoding into one implementation-specific state.

Restore tests use real factory-created checkpoints and vary caller-owned budget
or dimensions. One-byte-below rejection and exact-budget success check the
resource boundary; wrong-size rejection and original-size continuation check the
checkpoint/configuration ownership boundary. The rejected restore owns and drops
any partially created terminal internally, while the original source terminal
remains usable. These tests assert that public behavior without constructing
private native records, replacement serializers or ABI-invalid state.

The two materializer cases use the existing internal owned-write entry point
inside its existing test module. This is not an exported public runtime API, but
it is a real adapter operation with actual path/bytes and kernel filesystem
results; it does not inject a private intermediate state or add a new fault hook.
The permission case verifies mapping to the domain PermissionDenied error while
preserving unrelated inode contents and caller-thread signal state, then retries
successfully. Embedded NUL verifies InvalidCommand before C-string truncation
could change the selected path, followed by a successful independent write.
The directory owner restores permissions in Drop before removal. The test's
unprivileged execution assumption is explicit: root/capability bypass is not
proven by a chmod-0500 test, and an unexpected success correctly fails its oracle.

No application/domain dependency on infrastructure or fixture behavior is added.
Production configuration, cleanup policy and existing assertions remain unchanged.
These are bounded contract tests, not a final native/performance/coverage gate.

## Bounded assessment of the remaining fullcontinuous01 gaps

The retained workspace export still reports image_materialize 141/189 lines,
process backend 136/158, spawn 115/122, spawner 66/78 and terminal projection
179/191. These are pre-addition fullcontinuous01 figures, not a prediction of the
running matrix02 or an exact classification of every uncovered line. The new
cases legitimately address ordinary input/permission/terminal-state gaps; they
do not establish universal reachability of remaining defensive branches.

Concrete limits from the actual code:

- Materializer signal-set/install/restore failures use valid allocated sigset_t
  pointers and SIG_SETMASK. No supported path/bytes input can request an invalid
  pthread_sigmask operation. Its EINTR/retry and syscall-error branches depend on
  kernel behavior, not an ordinary public value that these tests can select.
- Fork, thread creation, descriptor duplication and socket-pair failures can
  occur under OS resource pressure, but the current APIs do not deterministically
  target one selected call with an error. Exhausting shared host resources would
  not by itself establish the intended rollback branch or be a bounded substitute
  for an explicitly designed failure seam. No such seam is proposed here.
- The materializer's waitpid ownership-loss recovery explicitly describes
  competing reapers/NOCLDWAIT as prohibited backend conditions. Treating a test
  that steals the child as a supported public contract would misstate that
  precondition. Ordinary valid staging also does not guarantee fchmod/close or
  write-zero failures; actual filesystem failure environments would need their
  own bounded, source-bound evidence.
- Supervisor panic catch and disconnection/error arms are defensive paths. A
  normal caller cannot select an arbitrary private worker panic. Existing public
  collaborator-failure tests establish their own port contracts, not every
  unrelated private recovery arm.
- Rust color conversion rejects tags outside 0/1/2; the C bridge emits 255 only
  when Ghostty itself returns an invalid color enum. Rust underline conversion
  rejects values outside 0..5; the bridge copies the engine's typed underline.
  The supported Rust terminal API supplies VT input and valid checkpoints, not
  arbitrary FFI structs. Malformed VT is parser input, not authorization to claim
  an ABI-invalid enum was produced. These error guards protect against native
  contract violations; no deterministic supported input reaching them has been
  demonstrated. Similar invalid Unicode scalar guards require engine-contract
  evidence, not fabricated private codepoint arrays.

These are limits of the existing bounded validation interface, not proof that
all OS/native failures are impossible in production. They do not justify source
exclusions, deleting guards, reducing thresholds, weakening assertions or marking
zero regions covered. With the present scope, a remaining 100% failure must stay
a failure; final counts must come from the completed matrix02 and its unchanged
source/tool evidence. Native C, platform variants and independent mechanical,
adversarial and performance acceptance remain separate requirements.
