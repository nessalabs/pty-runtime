# Abrupt public Runtime-owner death and high descriptor closure

The `guardian_owner_death` example constructs the public `Runtime`, uses its
packaged guardian adapter to spawn a real Python workload on a raw PTY, and stays
alive until the external driver kills it with SIGKILL. This bypasses Rust Drop,
normal shutdown, and cancellation. The driver is outside the owned session.

The workload creates separate foreground and background process groups, an entrant
in the sentinel's group, and an entrant in the guardian's group. All ignore HUP
and TERM; the foreground group is installed with `tcsetpgrp`. A deliberate
`setsid` child escapes the session. Each has a private driver socket for readiness
and safe cleanup. Before owner death, the driver verifies actual workload PID,
PTY attachment, session/group membership, and distinct sentinel/guardian/workload
PIDs. After death, an OS census must find no live member of the original session
within ten seconds, including dynamically created cleanup anchors. Zombie and
exiting states are excluded from the live census. An unrelated outside-session
process and the deliberately escaped child must survive; the escaped child must
answer a socket ping. This explicitly documents that deliberate session escape is
outside the promised cleanup scope.

The driver also inherits a pipe writer at descriptor 300 or above with CLOEXEC
explicitly cleared. After spawn, the Rust owner closes its copy. Before owner
death, the workload must report EBADF for that descriptor and the external reader
must see EOF while the workload and helpers remain alive. Together these prove
that the helper/workload exec path does not retain the high non-CLOEXEC writer.
This is stronger than testing duplicated descriptors whose CLOEXEC bit is set.

Reproduce from the repository root:

```sh
cargo build --offline --locked --no-default-features --example guardian_owner_death
python3 scripts/guardian/owner_death.py target/debug/examples/guardian_owner_death --iterations 25
```

Raw JSONL records contain platform, every workload role's PID/SID/PGID, measured
cleanup duration, expected escape/outside survival, and binary/driver SHA-256.
These are focused guardian proofs using the real public Runtime with projection
disabled; they do not replace the full release soak or fault-injection matrix.

Recorded result: 25/25 macOS and 25/25 Linux iterations passed. Maximum measured
live-session cleanup was 22.8 ms on macOS and 64.5 ms on Linux.
