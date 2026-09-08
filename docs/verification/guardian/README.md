# Packaged guardian qualification

The default Unix adapter now embeds a separately built Rust executable. Each
admitted workload has a session-leader sentinel S and a workload-parent guardian
G. S and G use independent owner channels and a private peer channel. The public
process ID and exit record identify the actual workload W, never either helper.
Owner admission grants execution only after both helper channels are ready.

The helper is a separate Cargo workspace with a pinned lockfile. The native build
hook builds for Cargo's TARGET, checks the executable architecture, and embeds
its exact bytes. The adapter stages those bytes in a private executable directory.
An explicitly supplied bundled image must match those same bytes. The build-time
`PTY_RUNTIME_GUARDIAN_IMAGE` option supports an externally signed matching image;
cross-target and signed-distribution qualification are separate release checks.

Only a verified member anchor signals group zero. No owner transport operation
signals a discovered numeric PGID. G can additionally signal its unreaped actual
child. Metadata discovery is a bounded hint; unknown or incomplete discovery
retains cleanup ownership and retries. Helpers retain the SID while inspecting
same-session groups. Descendants in both helper groups require a temporary
successor; the former guardian parks until the successor acknowledges takeover
through S and starts retirement. This admits two persistent helper processes and
up to three temporary helper/anchor processes during overlapping cleanup. An
unbounded stream of process-group migration/forks or deliberate session/credential
escape is outside the qualified descendant model.

On macOS the surviving controlling-session leader can suppress master EOF even
without a remaining slave FD. A bounded, non-destructive census after W exits
retires an empty session. A live descendant preserves its configured drain window
on both platforms; timeout reports Truncated separately from W's actual exit.
Cleanup continues after public drain completion. Unknown cleanup state never
becomes a fabricated successful workload exit or forgotten live owner.

Focused tests passed on macOS arm64 and Linux x86_64: process contract (7),
reader-failure cleanup (1), retained-handle descriptor teardown (1), host contract
(3, plus one isolated fixture), concurrent pressure (4), and public helper-loss
classification (2). `scripts/guardian/probe.py` passed 18 cases on each platform:
actual exit/input, S/G kill and G abort, workload entrants in either/both helper
groups with normal and damaged cleanup, owner EOF before/after execution, and
successor/G/S death before handoff acknowledgement. An outside-session process
survives the complete probe. The handoff cases stop S to make the failure window
deterministic. These focused runs do not replace the final reviewed-revision gate.

The new `guardian_resources` infrastructure example uses the actual default
packaged adapter. `scripts/guardian/resources.py --adapter PATH` measures baseline
owner, live owner, each helper, each sleeping workload, and owner after shutdown.
The checked-in JSONL includes platform and executable SHA-256. macOS 1/32/128
session samples used 2/64/256 persistent helpers, with aggregate helper RSS
2,768,896 / 89,260,032 / 357,138,432 bytes and 14/448/1792 helper descriptors.
Owner descriptors were 3 initially, 14/262/1030 while live, and 3 after shutdown.
The five-second samples observed zero aggregate CPU seconds at `ps` resolution;
this is not a claim of mathematically zero CPU use. macOS PSS was not measured.
RSS is a sum of resident samples, not unique physical memory or a peak bound.
These measurements exclude native terminal/projection work and transient anchors,
and therefore do not establish full-runtime 128-session release capacity.

Linux x86_64 samples used a task-shell soft descriptor limit of 4096. At 1/32/128
sessions, helper RSS was 3,006,464 / 96,452,608 / 386,265,088 bytes, aggregate PSS
484,352 / 6,697,984 / 24,799,232 bytes, and helper descriptors 14/448/1792.
Owner descriptors were 3 initially, 14/262/1030 live, and 3 after shutdown.
An initial run with the host's soft limit of 1024 rejected admission at the
128-session size with a typed process I/O error. Thus 128-session admission also
requires adequate host descriptor limits; the successful census changed only the
test shell's limit. The five-second Linux idle samples observed zero aggregate
CPU seconds at `ps` resolution, with the same measurement limits as macOS.

The two per-session helper event loops are an explicit process-isolation cost in
addition to the dedicated reader and shared host supervisor. Their FD, memory and
process costs must remain visible in admission and release performance decisions.
Discovery-error/fork-exhaustion injection, cross-target packaging, signed image
deployment, and sustained fork/migration churn are not established by this census.
