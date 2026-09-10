# Foreground cancellation through an in-session guardian

Status: implementable design with an isolated mechanism prototype; not an implemented
runtime feature or a passed G1 gate. No production files changed for this study.

Recommend a small, freshly executed Rust guardian per admitted PTY. The guardian
creates the session and workload, then creates a temporary process-group anchor
when cancellation needs to signal a foreground job. The anchor joins the group,
checks its own session and foreground membership, and calls `kill(0, SIGTERM)`
and, if needed, `kill(0, SIGKILL)`. It never signals a cached numeric foreground
PGID from outside that group.

The mechanism passed six deterministic cases on both macOS arm64 and Linux
x86_64, with 25 repetitions per platform. This establishes the group-control
primitive. Packaging, full runtime integration, guardian failure handling, and
resource qualification still require implementation and review.

## Why a member anchor works

A new process can join an existing group only within its session. A group's ID
cannot be reused while a member remains. A helper that has successfully joined
and verified its own session therefore provides a lifetime bound for that group.
Sending to group zero uses the caller's current group; it does not repeat an
external numeric-PGID lookup. These are POSIX process-group and process-creation
contracts. [POSIX setpgid](https://pubs.opengroup.org/onlinepubs/009604599/functions/setpgid.html),
[POSIX fork](https://pubs.opengroup.org/onlinepubs/9799919799/functions/fork.html).

The inspected Linux implementation holds its task-list lock through the
same-session check and membership change. Its group-zero signal path uses the
caller's group under that lock.
[Linux setpgid](https://github.com/torvalds/linux/blob/28924df2a08f440c73991b83028032c901de2ae4/kernel/sys.c#L1114),
[Linux group signalling](https://github.com/torvalds/linux/blob/28924df2a08f440c73991b83028032c901de2ae4/kernel/signal.c#L1569).

On the inspected XNU source, `setpgid` checks the target's session, releases a
group reference, and calls `enterpgrp`, which obtains the target again. Do not
base our proof exclusively on that first check: after joining, the anchor must
verify `getsid(0) == expected_session`. It is single-threaded and its trusted
parent never moves it afterward. XNU's group-zero signal path obtains the
caller's actual group reference before traversing it.
[XNU setpgid](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/bsd/kern/kern_prot.c#L571),
[XNU membership change](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/bsd/kern/kern_proc.c#L2824),
[XNU group-zero signalling](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/bsd/kern/kern_sig.c#L1675).

This is a design inference from those contracts and implementations, supported
by the tests below. The prototype did not force system-wide PID wraparound.
It instead exercises destroyed groups, cross-session targets, a departed group
leader, and a foreground change at the join boundary.

## Process topology and actual workload identity

```text
runtime owner
  └─ guardian G: direct child, session leader, own group G
       ├─ workload W: actual executable, initial group W
       │    └─ ordinary shell-created foreground job group F
       └─ anchor A: created only for cancellation, joins F
```

The guardian must be the workload's parent so it can reliably collect the
workload's actual exit and reap anchors. Making the guardian an untracked child
of an arbitrary workload would let that workload's `waitpid(-1, ...)` interfere
with cleanup; when the workload exits, that topology also loses direct reaping
ownership of the helper.

The public process ID is W, never G. A bounded startup handshake reports W only
after executable startup succeeds. The guardian sends a distinct workload-exit
record containing the real `waitpid`/`waitid` result. Guardian termination is a
separate supervision event; it must never be translated into workload exit.
The isolated prototype reports different G/W IDs and forwards actual workload
exit code 42 while the guardian exits zero.

G creates the controlling session and terminal. W receives only the intended
stdin/stdout/stderr child endpoint and explicit command/environment data. G
then closes its copy of the child endpoint and retains an ioctl-only reference
to the host endpoint. Anchors inherit no child endpoint. This is necessary to
avoid manufacturing a descendant-held-output condition merely by keeping a
guardian alive. The prototype keeps child-endpoint stdio open and therefore
does **not** establish output-drain behavior.

W is initially a group leader rather than the session leader. Interactive shell
job control and terminal queries must be tested with this topology. On macOS,
keeping G as session leader can delay the terminal revocation previously caused
by W's exit; actual W exit and bounded output drain must remain independent.

## Bounded cancellation protocol

1. Owner admits one cancellation generation independently of input/projection.
   It sends a fixed-size control record to G; duplicate callers coalesce.
2. G samples the terminal's foreground PGID and forks A. A has no runtime or
   caller callbacks and no terminal input/output responsibility.
3. A calls `setpgid(0, sampled_group)`. A failed join reports a typed race/missing
   target; it never falls back to `kill(-sampled_group, ...)`.
4. A verifies its own SID equals G's SID and its own PGID equals the terminal's
   current foreground group. A mismatch causes exit without sending signals.
5. A ignores TERM/HUP/job-control stop signals and reports pinned readiness.
   On the TERM command it calls `kill(0, SIGTERM)`, keeping itself alive to hold
   membership throughout the grace interval.
6. G signals W's original group and W itself while retaining unreaped W identity.
   It must not reap W and then reuse its numeric PID for a later control.
7. At escalation, G checks whether the foreground has changed. Retain the old
   anchor while admitting at most one anchor for the newly current group. Ask
   both applicable anchors to call `kill(0, SIGKILL)` and signal unreaped W as
   required. Each anchor's own death is expected and is reaped by G.
8. If an anchor dies early, discard its control generation and identity. Never
   substitute a numeric-PGID kill. A fresh join/verification can be attempted
   within a finite retry/control deadline; exhaustion is an explicit control
   failure, not successful cancellation or a fabricated exit.

Two simultaneous anchors per session suffice for the sampled TERM group and
latest sampled KILL group. Rapid foreground churn can exhaust a bounded retry
policy; this must be visible rather than promising atomic cancellation of every
group that has ever existed. A foreground change after verification cannot make
the pinned group ID refer to an unrelated group. It can change which group is
currently foreground, which is why escalation samples again.

G runs a single nonblocking control/exit loop with kqueue on macOS and pidfds on
Linux. Fixed-size pending status records and cancellation state must remain
bounded even when the owner stops reading. Actual child reaping and anchor
cleanup cannot wait for a status write to become writable. The owner's existing
shared supervisor and dedicated readers remain; raw PTY bytes do not pass
through G.

## Owner loss and failure protocol

Use a private bidirectional socket with a fixed protocol version and a workload
lifetime/generation on every record. Reserve at most one startup record, one
workload-exit record, one failure record, and two anchor-result records per
session; bound command metadata by the existing command limit. Coalesce TERM
and immediate-KILL intent instead of accumulating commands. Drop stale records
by generation without dropping ownership of their children.

Owner-channel EOF means immediate cleanup, regardless of the configured graceful
timeout. G anchors/signals the current foreground group, kills unreaped W and
its anchored initial group, reaps W and all anchors, closes its host-endpoint
reference, and exits. W and every anchor must close the owner's control-channel
endpoints before exec or their main loop; otherwise owner death would not
produce EOF. Protocol corruption has the same cleanup consequence and a typed
supervision failure if the owner is still reachable. A failure before workload
exec must clean any partially created W before completing failed admission.

G is always in its own group G. Normal workload cancellation must **never** call
`kill(-guardian_pid, ...)` or use guardian status as workload status. The owner
requests W/F cleanup over the control channel. Only after child cleanup has
completed may it retire the guardian normally. Unexpected guardian loss remains
a distinct fault requiring additional qualification: owner EOF is handled by a
live guardian and does not prove cleanup after the guardian itself is killed.
A Rust unwind guard can cover ordinary helper panics, but SIGKILL/abort cannot
run that guard. Keep this explicit in the implementation review rather than
adding an unsafe numeric workload-PID fallback.

## Packaging as a Rust library

Build a small separate Rust helper for the Cargo target and embed the resulting
image in infrastructure. The helper can be an excluded Cargo package with its
own pinned dependency set and reproducible build command. Do not accidentally
embed the build host's architecture when cross-compiling.

The default adapter materializes the immutable embedded image in an exclusively
created private directory, using no-follow/create-new operations and private
executable permissions. It launches that image with a versioned inherited
control channel and explicitly mapped descriptors. Also allow a verified,
caller-supplied bundled helper path for signed macOS application packaging.
Neither path requires a compiler or an installed helper executable at runtime.
Cleanup removes the private image and directory after all guardians exit.

Fresh exec is a requirement, not a convenience. An indefinite Rust guardian
inside a multithreaded parent's `pre_exec` closure would inherit allocator,
runtime, and arbitrary host library state. POSIX permits only async-signal-safe
operations there until exec, and current POSIX no longer promises that `fork`
itself bypasses unsafe atfork handlers. A fresh single-threaded helper can use
ordinary Rust to manage its fixed state and then fork short anchor children.
[POSIX fork and _Fork](https://pubs.opengroup.org/onlinepubs/9799919799/functions/fork.html).

This adds one bounded guardian process per PTY and up to two temporary anchors
during cancellation. It is an explicit architecture/resource change, not a way
to hide a per-session waiter in another accounting category. Measure helper
resident/PSS/charged memory, descriptor count, idle CPU and launch latency at
1/32/128 sessions before retaining the ADR's current capacity/default claims.
Signed/notarized and sandboxed macOS embedding must execute the actual packaged
helper in qualification; the standalone prototype does not prove that path.

## Alternatives considered

| Option | TERM/KILL and reuse safety | Decision |
| --- | --- | --- |
| `tcgetpgrp` followed by `killpg` | The sampled number can be recycled before signalling | Reject |
| Read SID/process metadata, then numeric group kill | A second lookup still leaves a race | Reject |
| Kernel PTY signal ioctl | Linux `TIOCSIG` accepts INT/QUIT/TSTP, not TERM/KILL | Insufficient |
| Linux pidfd process-group signalling | Safe for a retained group-leader pidfd on Linux 6.9+; obtaining one after the leader was already reaped remains a problem | Useful optional optimization, not the portable baseline |
| Enumerate group members and signal process handles | Safe individual identities are possible on Linux, but enumeration misses concurrent entrants/forks; no qualified matching public macOS path here | Does not replace the group contract |
| Close all host endpoint references | Kernel hangup targets its own terminal/session references, but HUP can be ignored and closing truncates output | Does not implement TERM/KILL escalation |
| Fresh guardian plus verified member anchor | Works even after the group leader is reaped while members remain; uses current-group signalling | Recommended portable baseline |

[Linux PTY signal implementation](https://github.com/torvalds/linux/blob/28924df2a08f440c73991b83028032c901de2ae4/drivers/tty/pty.c#L188),
[Linux pidfd group flags](https://man7.org/linux/man-pages/man2/pidfd_send_signal.2.html).

## Executed prototype evidence

Durable prototype source and raw files are preserved under
[`docs/verification/foreground-prototype`](../verification/foreground-prototype/):
`anchor_probe.c`, `macos-results.jsonl`, `linux-results.jsonl`,
`macos-repeats.jsonl`, `linux-repeats.jsonl`, and platform metadata JSON files.
They are scratch evidence, not production implementation; preserve them with the
review evidence before relying on this study as a durable release record.

Source SHA-256:
`7b20bc2b905317b6d0e2625e15a4f7a8fabb0598f9bc6d7c4b43c2069d2d5225`.

```sh
cc -std=c11 -Wall -Wextra -Werror work/foreground-control/anchor_probe.c \
  -o work/foreground-control/anchor_probe
work/foreground-control/anchor_probe
# Linux uses the same source and flags, with -lutil added.
```

| Case | macOS arm64 | Linux x86_64 | Assertion |
| --- | --- | --- | --- |
| Cooperative foreground | 25/25 | 25/25 | Anchor joins/validates; TERM produces actual job exit 17 |
| TERM ignored | 25/25 | 25/25 | Group remains live after TERM; own-group KILL terminates it |
| Group leader already reaped | 25/25 | 25/25 | Remaining member can still be joined and killed safely |
| Group destroyed before join | 25/25 | 25/25 | Join rejected; no signal sent |
| Group belongs to another session | 25/25 | 25/25 | Join rejected; no signal sent |
| Foreground switched before verification | 25/25 | 25/25 | Membership succeeds but foreground verification rejects; no signal sent |

Every case also checks that a separate background group remains alive, that
workload and guardian PIDs differ, and that actual workload exit 42 is collected.
All fixture-owned children are explicitly waited. There were 150 cases and zero
failures per platform, plus a separately saved six-case initial run per platform.

macOS: Darwin 25.6.0, macOS 26.6 arm64, Apple Clang 21.0.0.
Linux: executed in Box `bx_98zpseyu`, kernel 6.8.0-117-generic x86_64,
glibc 2.39, GCC 13.3.0. Linux files also reside in
`/tmp/pty-foreground-proof`; the VM was left running for other qualification.

## Required proof before replacing the production adapter

- Rust helper build/embedding, version handshake, wrong-image and exec-failure
  handling, and signed macOS bundle execution.
- Real interactive shells creating/foregrounding jobs, stopped jobs, shell
  resize handling, and TERM/KILL against both W and F in one cancellation.
- W PID/status propagation through the application; helper exit must remain a
  supervision failure. The prototype uses synthetic sibling groups rather than
  a real shell-created foreground descendant.
- Guardian holds no child endpoint after launch; inherited endpoint drains,
  natural workload exit, and shutdown remain correctly separated on both OSes.
- Anchor death, guardian panic/failure, owner death, protocol backpressure,
  concurrent foreground changes, and every partial-spawn cleanup boundary.
  In particular, unexpected guardian death can orphan W: do not claim automatic
  cleanup until a qualified failure mechanism exists. A guardian crash is not
  evidence that the workload exited.
- Child credentials may change signalling permissions. Group-call success can
  include only permitted members; it is not proof that every descendant died.
  Keep real exit/supervision outcomes and the existing deliberate-escape limit.
- Complete resource/performance measurements including helper processes and
  the mandatory lifecycle/reconnect/soak gates.

The recommendation supplies a concrete route to the original foreground-group
requirement. It does not mark that requirement complete or revise it away.
