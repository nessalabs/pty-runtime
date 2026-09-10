# Independent sentinel/guardian production design review

Status: conditional design support; **not production approval and not G1 closure**.
Reviewed 2026-09-08: `work/guardian-failure/DESIGN.md`, both C probes, stored
macOS/Linux results and source hashes, and `foreground-control-design.md`.
This review did not rerun the probes or modify production code.

## Conclusion

Two independently supervised helpers provide a plausible recovery path after
one helper dies. The important property is continued membership in the owned
session, followed by a temporary child's verified membership in each group it
signals. Process enumeration supplies candidates; it never grants authority to
signal a numeric PID/PGID. This is stronger than retaining a stale foreground
number and avoids depending on terminal metadata after session-leader death.

The C experiments support that narrow claim. They do not establish complete
production enumeration, failure-path ownership, packaged Rust startup, bounded
resource use, or application-level exit reporting. Those remain acceptance
requirements, not implementation details that can be postponed after G1.

## Evidence and identity argument

Each platform has 25 leader-death records and 75 sweep records (25 per mode).
The reviewed leader records preserve SID and report successful anchor joins;
the sweep records report no live fixture workload remaining and survival of an
outside-session process. Mode 0 reports workload status; modes 1/2 correctly
report guardian status. These are finite real-shell foreground/background jobs
that ignore HUP/TERM. They do not test arbitrary group migration or exhaustion.
Both source hashes match the platform metadata:

- `leader_death_probe.c`: `2bba9e38362eb05b3f4650f90e4d7fb613910ee8fdafd0a5997734021439d00a`
- `session_sweep_probe.c`: `eb897625c50bb3024131fa8ed58d1a060b7f3a721a8f8b0fd8c897500e8a9c2e`

The Linux metadata also contains a historical `cases: 150` summary and separate
`source_sha256`; neither should be presented as the record count or either
current probe's hash. Use the explicit files and `sources` mapping above.

A surviving helper also prevents SID reuse, including after the session leader
is gone. XNU's PID allocator rejects IDs found in the session hash, as well as
process and process-group IDs. [Pinned XNU allocator](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/bsd/kern/kern_fork.c#L969).
Linux attaches session membership using `PIDTYPE_SID` and retains the PID object
until no task lists of any PID type remain. [Pinned Linux fork](https://github.com/torvalds/linux/blob/28924df2a08f440c73991b83028032c901de2ae4/kernel/fork.c#L2516),
[pinned Linux PID lifecycle](https://github.com/torvalds/linux/blob/28924df2a08f440c73991b83028032c901de2ae4/kernel/pid.c#L402).
This supports the live-SID premise; it does not make metadata snapshots atomic.

The signal capability remains `setpgid(0, candidate)` followed by
`getsid(0) == owned_sid` and then `kill(0, SIGKILL)` in the anchor itself.
Post-join verification matters on XNU; an earlier check of the candidate does
not substitute for it. There must be no numeric-group fallback when any step
fails. A successfully joined anchor is itself a member that prevents that group
from disappearing and being reused before its group-zero signal.

## Acceptance blockers

| Priority | Requirement | Concrete failure otherwise |
| --- | --- | --- |
| P1 | Complete, error-aware enumeration with retained ownership | A truncated or unreadable scan becomes “empty,” allowing the last helper to exit with live workload. |
| P1 | Explicit retirement of helper-owned groups | An ordinary same-session descendant joins a helper PGID and survives a sweep that excludes it. |
| P1 | Bounded protocol writes independent of reaping and cleanup | A stalled owner fills the status pipe and prevents the survivor from cleaning or reaping. |
| P1 | Startup/partial-failure descriptor proof | W or an anchor inherits an owner/liveness writer and suppresses EOF recovery indefinitely. |
| P1 | Explicit actual-exit and supervision-loss semantics | G's death is exposed as W's exit or a fabricated success; W may still be alive. |
| P1 | Updated resource/architecture decision with measurements | Two per-session helper processes are omitted from capacity and memory claims. |

### Discovery and continued ownership

Production must distinguish `Gone`, `Present`, and `Unknown`. A metadata error,
permission error, parser error, truncated buffer, failed directory iteration,
anchor-fork failure, or exhausted scan budget is not proof of absence. The
prototype's fixed 32,768-PID / 64-group arrays, eight passes, and permissive
`is_live` filtering must not be copied as a disappearance proof.

Either establish a complete enumeration capacity with detectable truncation or
implement bounded resumable discovery. For every platform, document what proves
a scan complete and how concurrent changes trigger another pass. Exhaustion must
produce a visible cleanup failure while retaining a live helper and its ledger;
returning an error must not drop ownership. This also applies to `fork` EAGAIN,
control-channel loss, and terminal errors. A failed group join means retry or
revalidate, not “the session is empty.”

Finite repeated scans do not guarantee completion against an unbounded forker
or group migrator. Preserve that explicit contract boundary. It is distinct
from the required recovery of finite ordinary shell jobs.

### Helper groups and final retirement

Normal W setup in a separate group is insufficient protection: an ordinary
same-session descendant can subsequently join a helper group. Track both helper
identities and groups. Never sweep a live peer's group as a candidate; once it
is known dead, its remaining group members must become cleanup candidates.

The proposed implementation response—finish other groups and have the final
survivor issue group-zero SIGKILL—can address descendants in its own group.
It needs an explicit protocol distinction between “other groups verified clean;
final retirement committed” and observed helper termination. Sending a record
before final retirement is not evidence that retirement already happened.
Prove both helpers cannot retire while leaving the other helper group excluded,
and prove an unexpected second failure is reported within the stated failure
model. Test descendants joining each helper group, including peer loss during
cleanup. Ordinary `exit()` by the final helper is insufficient.

### Protocol, startup, and status

Use bounded outgoing records with partial-write offsets and explicit handling of
EINTR/EAGAIN, peer EOF, and malformed/version-mismatched messages. A status write
must never block wait/reap or cleanup. Bound the combined number of anchors when
normal cancellation and emergency sweeps overlap; “one per sweep” alone is not a
whole-session bound.

Both readiness acknowledgements must precede W execution. W exec success must
be distinguishable from helper readiness. Enumerate ownership of every endpoint
at each spawn stage and close unrelated descriptors, including descriptors above
255 (the prototype's closure range is not a production solution). Test failure
at every partial-start stage, owner EOF before and after W exec, and inherited
endpoint closure in W and anchors. Only async-signal-safe setup may run between
host fork and exec; the Rust supervision loop starts in a fresh executable.

Only G's actual wait of W supplies W's exit status. After G loss, cleanup may
succeed without recovering that status. Surface supervision loss and unknown
workload exit explicitly. S status is never W status; neither EOF, a successful
kill syscall, nor process disappearance reconstructs wait status. Host reaping
must respect actual parentage, including orphaned descendants. Keep PTY draining
independent of helper completion, as the prototype's earlier hangs demonstrate.
Also retest natural W exit and EOF: preserving a session leader changes terminal
lifetime behavior relative to the former direct-child topology.

## Clean Code / SOLID assessment

The topology is justified by fault isolation, but it should not become a large
role-switched OS loop coupled directly to application events. Keep these
responsibilities explicit:

1. A small versioned wire codec validates bounded messages and maps them to
   internal events. It knows no domain lifecycle policy.
2. A supervisor state machine owns topology roles, child identities, pending
   statuses, retry budgets, and permitted retirement transitions.
3. Platform process discovery returns candidate metadata plus completeness and
   error state. It cannot signal candidates or decide lifecycle success.
4. An anchor operation establishes membership and performs group-zero signalling.
   Its failure cannot silently become a weaker kill strategy.
5. The runtime adapter translates supervisor results into existing application
   lifecycle events and continues independent terminal draining.

Prefer concrete small types for these boundaries over a generic process-control
framework. Share the wire protocol between host and helper, keep platform code
behind a narrow adapter, and avoid duplicating status/lifecycle meanings across
S and G. Distinguish workload identity from helper identity in types or explicit
fields so an accidental substitution cannot compile unnoticed or pass tests.

An interface for discovery is valuable because tests can inject truncation,
unknown metadata, and interrupted scans. Tests should assert observable
ownership/cleanup behavior rather than mirror every state-machine branch.

## Required production qualification

Run the actual packaged Rust helper on macOS and Linux, with real interactive
shell foreground and background groups and an outside-session survivor. Include
S SIGKILL, G SIGKILL/abort, owner EOF, failure during startup and cancellation,
stalled status readers, injected discovery errors/truncation, anchor admission
failure, both helper-group entrant cases, and accurate versus unavailable W
status. Confirm no live ordinary workload or retained terminal endpoint remains
on successful cleanup and that incomplete cleanup retains its owner.

Measure complete runtime process/thread counts, descriptors, idle CPU, RSS/PSS
where available, charged memory, and launch latency at 1/32/128 sessions. At 128
sessions this topology alone adds 256 persistent helpers, before workloads,
readers, and temporary anchors. Qualify process-limit admission as well as memory.
The previous one-helper resource comparison cannot establish this topology's
capacity. Record the explicit architecture deviation before retaining default
capacity claims; helper wait loops still consume per-session OS resources.

Validate target-architecture embedding, version/exec failures, private executable
materialization, and the supported signed macOS packaging path. A compiler-built
standalone C probe is useful mechanism evidence, not distribution evidence.
Only after these results and the integrated lifecycle checks pass can this
review support production acceptance or G1 closure.
