# Independent packaged guardian correctness review

2026-09-08. Reviewed the actual Rust helper and default process adapter, including
launch/image materialization, registration, host supervision, shared protocol,
discovery, anchors, cancellation, S/G recovery, successor retirement and draining.
This supplements the production-design and architecture reviews; the reviewer did
not author production changes. No full gate was run during concurrent work.

Disposition: no open concrete P1/P2 findings in the final reviewed source after
the drain scheduling fix below. Qualification limits remain explicit.

## Resolved P2: artificial delays could exhaust an empty session's drain window

The public `raw_completion` test failed at the no-descendant SIGTERM case: W's
actual signal status was correct, but drain was Truncated instead of EOF at a
40 ms deadline. G's post-exit loop slept 10 ms after every 128-PID census chunk.
Unrelated host process count therefore added deliberate delays before an empty
controlling session could retire and expose EOF.

The author added `Drain::delay`: an in-progress bounded scan resumes immediately;
observed live members or unknown metadata retain the retry delay. G uses that
delay instead of unconditional 10 ms sleeps. Independent source inspection and
the same public test confirm the fix. This does not promise EOF under arbitrary
host scheduling delay or turn a timed-out drain into success.

The test's separate macOS descendant expectation was also obsolete. S now retains
the controlling SID after W exits, so a live `sleep 1` descendant holds the slave
beyond the configured 40 ms on both platforms. The reviewer changed that assertion
to Truncated while preserving W's actual exit 23. The complete test now passes.

## Independent changing-foreground regression

Added `process_foreground_switch.rs` using the default public adapter and a
self-executed Rust fixture. W creates two separate groups inside the owned SID,
sets the first foreground, and moves foreground to the second when TERM arrives.
Both descendants ignore TERM/HUP; W stays alive through the grace interval.

The regression proves the switch actually occurred, the reported W exit is
SIGKILL without supervision failure, both former/current foreground descendants
are gone or dead after cleanup, and a separately owned outside-session process
survives. The wrapper has an eight-second deadline. Fixture fork children use
only async-signal-safe libc calls and never return to the Rust harness.

## Independent retirement probes

Expanded `scripts/guardian/probe.py` from 18 to 20 cases. The two new cases stop C,
allow S to acknowledge its takeover, stop S again and resume C. They require old
G's observed retirement before injecting C or S death; elapsed time alone is not
treated as acknowledgement evidence. Both require complete session cleanup.

A queued Finish can authorize the surviving group to retire before it notices
the other helper's death. Consequently a separate Fault frame is not mandatory:
the host also classifies channel EOF without that channel's Retiring as supervision
loss. The probes check that same distinction. They do not reinterpret final
retirement intent as proof that a process has already exited.

All 20 probes passed independently on macOS against the rebuilt Rust release
helper, SHA-256 `378b8dc3abacd200cd218da5afcfc7bf9246f9f568d69b90761a2386c40ff3eb`.
These include real workload status/input, S/G death and G abort, either/both helper
group entrants, owner EOF before/after execution, all three pre-ack actor failures,
the two post-ack failures, and an outside-session survivor. The default packaged
adapter was independently exercised by the public Rust integration tests below.
This reviewer did not rerun Linux; the author's Linux results remain attributed
to the author rather than relabeled as independent execution.

## Ownership and authority assessment

- Host launch maps only pinned descriptors in syscall-only pre-exec setup and
  starts a fresh helper executable. Both readiness records precede Execute.
  Workload `Command::spawn` distinguishes actual exec success from helper readiness.
  Parent copies of child channel endpoints close before admission can wait on EOF.
- Before native creation, fallible allocations/descriptors have ordinary RAII.
  After S creation, Guardian owns S, both channels and a separate cleanup master
  descriptor. Pending registration and registration-error paths retain that owner.
  Admission is released only after the pending/owned process cleanup path drops.
- Anchors are forked only after parent channel setup. A retained direct-child
  ledger survives exit-watch failure. A member changes only its own PGID,
  verifies its own SID after joining, and signals group zero. Numeric candidate
  PID/PGID metadata never becomes signal authority. G's additional numeric signal
  is restricted to its exclusively owned, unreaped direct W child.
- Discovery separates unknown from absent; full macOS inventory, failed metadata,
  failed anchor creation and incomplete scans cannot prove an empty session.
  Cleanup retries with ownership retained. PID reuse between metadata operations
  cannot bypass the anchor's actual-membership verification.
- Normal cancellation has at most two anchors; cleanup waits for those ledgers
  before starting its sweep. Helper-owned groups are reserved while their recovery
  actor lives, then become candidates or retire through their own group-zero kill.
  Both populated helper groups use the successor handshake rather than simultaneous
  exclusion/retirement. Old G's private monitor closes unrelated endpoints before
  parking; setup failure terminates its group instead of retaining duplicate links.
- Only G's actual wait produces W status. S can forward it, but S/G status, EOF,
  kill success and metadata disappearance never reconstruct a W exit. Host status
  and fault handling remain distinct, including unexpected channel loss.
- Protocol IO is nonblocking and bounded. Failed writes do not suppress final
  readable status/Retiring records. Draining and discarded reads remain independent
  from helper completion, so endpoint backpressure cannot gate cleanup ownership.

## Independent execution and limits

- `cargo test --locked -p pty-runtime-infrastructure --test process_guardian_failures --test process_contract --test process_foreground_switch`:
  seven contracts, two helper-loss cases, one changing-foreground case and its
  isolated fixture entry passed.
- `cargo clippy --locked -p pty-runtime-infrastructure --test process_foreground_switch -- -D warnings` passed.
- `cargo test --locked --test raw_completion` passed after the drain fix and stale
  descendant expectation correction. An intermediate attempt was compilation-blocked
  by concurrent terminal enum changes; it is superseded by this successful run.
- `cargo build --manifest-path helpers/guardian/Cargo.toml --locked --release --bins`
  and the expanded 20-case helper probe passed.

No discovery-error or fork-exhaustion injection was performed in this review.
Those paths were inspected for retained ownership, not promoted to tested recovery.
Resource exhaustion may therefore remain visibly incomplete rather than finish
within the ordinary finite-workload cleanup deadline. Sustained fork/group churn,
deliberate session/credential escape, simultaneous loss of all remaining recovery
actors, signed deployment and the full target/resource release matrix remain
outside this focused correctness result. The author's census measures topology
cost; it does not establish an aggregate native-runtime memory or process ceiling.
