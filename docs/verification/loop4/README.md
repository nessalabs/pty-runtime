# Loop 4 qualification in progress

Candidate 2 subsequently passed both mechanical gates with exactly matching
281-file source inventories: `macos-gate-candidate2` (95.58 seconds) and
`linux-gate-candidate2` (497.90 seconds). Both recorded unchanged sources. This
candidate includes the allocator capacity correction and reviewed load-harness
fixes. All five corrected attached-load trials passed; their raw evidence is in
`../release/load-attached-candidate2`. Remaining load modes and independent
performance-methodology review are still in progress. The deferred checkpoint
crash and coverage gaps remain unresolved.

The frozen source inventories in `macos-gate-attempt3/metadata.json` and
`linux-gate-attempt2/metadata.json` match exactly. Both mechanical gates passed,
with unchanged source during execution: macOS arm64 in 93.35 seconds and Linux
x86_64 in 260.03 seconds. Logs and complete inventories are retained beside those
metadata files. Earlier attempts remain as failure/diagnosis evidence.

These runs predate the later projection fault tests and native boundary test
gate entry in the working tree. They do not establish a gate pass for subsequent
edits. The final reviewed source requires another gate.

The same frozen macOS build completed the full 10,000 lifecycle and 100,000 attach
repetition counts and all 256 seeded race rounds. See
[`../release`](../release) for exact build and raw run evidence. Repetition ended
with the baseline eight owner descriptors, no remaining child or zombie at
quiescent samples, and zero asserted runtime reservations. Its 65.8-second
duration cannot establish a long-term memory plateau.

Release qualification remains incomplete. In particular, the terminal corpus
seed 201 reproduces a native process crash; the coverage readiness targets remain
unmet; and full load qualification, twelve-hour soak and remaining ADR evidence
must be completed and reviewed. A mechanical gate or a scoped native-bridge
coverage result does not discharge these requirements.
