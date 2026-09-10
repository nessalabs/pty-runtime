# Independent checkpoint crash-cleanup review

Reviewed by root, independently of correctness_review's implementation, on
2026-09-08. Scope: domain cleanup limits/report; infrastructure arena, inventory,
cleanup, directory ownership/locks, FileCheckpointStore construction and crash
regressions. The exact consolidated source manifest belongs to the loop-4 gate.

## Finding and resolution

P2: bounded scans restarted at the same live-owner prefix, while temporary
construction admitted another namespace when the scan was incomplete. Later
abandoned data could remain unexamined across repeated restarts. A finite
per-pass loop did not prevent unlimited new admission behind that prefix.

The constructor now rejects either incomplete arena enumeration or positively
identified unfinished abandoned cleanup with CapacityExceeded. Finite scan limits
are configurable; the default namespace scan ceiling is explicit. The new
actual-directory-order regression keeps a live prefix and a hidden abandoned
60-byte object, verifies repeated small scans/constructors cannot add namespaces,
then uses a larger finite scan to reclaim the object and admit. Independently
inspected the fix and reran the crash-cleanup, provider contract and adversarial
integration suites: 18 tests passed. The P2 is resolved.

## Ownership, security and bounded work

The private versioned arena has validated UID and exact 0700 permissions.
Initialization and reclamation serialize through a bounded arena flock. A fresh
namespace retains its own directory flock throughout store lifetime; cleanup uses
a separate open description and a nonblocking ownership lock. No PID-reuse
heuristic supplies liveness. Direct explicit-path construction also locks its
parent across mkdir and namespace-lock acquisition, covering the initialization
window inside a managed arena.

Directory enumeration owns a fresh descriptor/stream, distinguishes errno from
EOF, copies bounded names before the next read and closes on every return.
Namespace and object validation rejects symlinks, foreign ownership, unexpected
modes, hard links, nonregular objects and unknown names. Deletion is anchored to
owned descriptors and never recurses. Entry, namespace and logical-byte limits
bound a maintenance pass. Errors and partial cleanup remain explicit; successful
partial work is reported without estimating unexamined data as free.

The parent/arena trust requirement remains material: cooperative locking and
inode checks do not promise protection against a hostile same-UID actor replacing
names concurrently. The implementation does not claim portable inode-conditional
unlink or recovery of a prior owner's encryption key/process. Unknown and legacy
names are not silently adopted or deleted.

## DDD and organization

Domain owns finite cleanup policy and portable results. Filesystem names, errno,
flock, DIR streams and descriptor operations remain infrastructure concerns.
The focused arena, inventory, reclamation and directory modules have separate
responsibilities. The existing immutable store contract remains intact; no
application dependency on filesystem records or a generic repository was added.

No unresolved P1/P2 remains in this reviewed scope. Linux execution, the complete
mechanical gate and broader release resource/soak evidence remain separate checks.
