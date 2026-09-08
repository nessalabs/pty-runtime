# G3-12 bounded crash-abandoned checkpoint cleanup

Implementation and focused author validation, 2026-09-08, macOS arm64. Independent
review and the final frozen-revision gate are pending; this is not full G3 closure.

`FileCheckpointStore::temporary` now uses a private versioned arena under its
configured parent. A directory flock protects arena initialization/maintenance;
each store separately retains its namespace directory flock. A crashed owner
releases that lock through kernel descriptor teardown. Cleanup skips locked live
namespaces and never infers liveness from a PID. Explicit `new(path)` constructors
also lock their parent during mkdir/owner-lock establishment, including if a caller
places one inside the arena. Lock acquisition has a one-second admission deadline.

Domain `CheckpointCleanupLimits` validates finite namespace, total entry and
logical unlink-byte budgets. `CheckpointCleanupReport` counts examined/live/reclaimed
work and distinguishes incomplete arena enumeration from positively identified
abandoned work remaining. Every incomplete scan prevents new temporary admission,
even when all examined owners are live: an unexamined suffix may contain abandoned
data. The default namespace scan ceiling is 256 (admission requires observing EOF
within the configured bounds); callers may increase finite limits for larger live
populations. A known abandoned namespace exceeding deletion bounds or failing
validation/removal also prevents admission. Explicit maintenance accepts
larger finite budgets; reports never estimate bytes in unexamined namespaces or
claim that partial enumeration is complete. Remaining unexamined disk data is not
a reservation in the new owner's `CheckpointCapacity`.

All operations use anchored descriptors, O_NOFOLLOW, private ownership/modes and
single-link regular-file validation. Only exact adapter namespace names and exact
checkpoint/pending object names are eligible. Unknown entries remain in place and
produce explicit failure. There is no recursive removal or ciphertext decoding.
Named namespace identity is rechecked before rmdir; a concurrent normal Drop that
already removed the namespace is treated as gone, not failed reclamation.

The parent/arena retains the existing trusted-parent contract: it must not undergo
hostile same-credential concurrent replacement. Unix does not provide portable
inode-conditional unlink/rmdir. Existing symlink/replacement validation is preserved;
this implementation does not pretend to sandbox an adversarial process with the
same account and direct write access to the private arena. Unsupported filesystem
locking or validation errors fail explicitly. Directory locks are CLOEXEC; inherited
non-exec fork copies can conservatively keep an owner live until all copies close.

Normal teardown removes owned objects and namespaces. The empty private arena is
retained as one coordination directory per configured parent. Legacy directories
outside that arena and arbitrary named stores elsewhere are not automatically
discovered or deleted. Owner/key restart restoration remains excluded by ADR 0003.

## Executed focused checks

- `cargo test --locked -p pty-runtime-infrastructure --test checkpoint_crash_cleanup --test checkpoint_adversarial --test checkpoint_contract --test checkpoint_opaque_envelope`:
  18 tests passed (including one isolated crash-fixture entry).
- `cargo test --locked -p pty-runtime-infrastructure checkpoint --lib`:
  four existing partial-write, quota, orphan-charge and namespace-identity tests passed.
- `cargo clippy --locked -p pty-runtime-infrastructure --all-targets -- -D warnings`:
  passed after the final parent-lock refinement and all new tests.

New real-process tests hold two committed objects in a child, verify that cleanup
skips it while alive, deliver SIGKILL, then reclaim exactly 120 bytes/two objects
while another live owner's ciphertext remains readable. Partial cleanup removes
60 bytes, reports remaining abandoned work, and a subsequent pass finishes.
A live-prefix regression verifies repeated bounded scans cannot admit new owners
while hiding abandoned data; increasing the limit reclaims that data and admits.
Separate tests cover file/namespace/arena symlinks,
hard links, changed permissions, unknown names, interrupted empty construction,
and bounded refusal to initialize while a maintenance lock is held.

Linux execution, hostile filesystem fault injection beyond these cases, every
failure/cancel permit-release combination, and full release/soak qualification
are not claimed by this author run. Existing runtime/storage quotas and provider
semantics remain in force; cleanup counters describe maintenance work separately.

## Gate fixture contention refinement

The first recorded loop4 macOS gate failed the maintenance-lock test's immediate
nonblocking fixture acquisition after a temporary store had been dropped. That
assertion did not capture errno, so the historical error cannot be identified
uniquely. A deterministic macOS reproduction establishes the invalid assumption:
a concurrent fork can retain an open-file-description flock after the parent
closes its CLOEXEC descriptor, until the child execs or closes/exits. An independent
LOCK_NB then returns WouldBlock (macOS errno 35). This is expected kernel ownership,
not evidence that the production bounded constructor bypassed its lock.

The original maintenance test now builds a fresh empty private arena directly,
so its initial lock has no predecessor maintenance operation. It still requires
`new(path)` to fail within its finite deadline without creating a namespace while
maintenance holds the lock, and to succeed after release. A new isolated self-exec
regression deliberately forks a child that performs only async-signal-safe calls
and retains the lock. It asserts WouldBlock, verifies the production temporary
constructor returns CapacityExceeded without admission, then kills/reaps the
owned child and verifies successful admission. Isolation prevents the intentional
one-second inherited lock from retaining unrelated parallel fixture resources.
No production locking behavior was changed.

After this refinement, all 19 checkpoint integration tests and infrastructure
all-target/all-feature strict Clippy passed on macOS. The nine crash-cleanup tests
also passed 20 repetitions with eight test threads (180 test executions).
