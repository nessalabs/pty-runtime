# Resumed runtime qualification

Release acceptance remains incomplete. This continuation starts at `46dbec6`
and preserves the uncommitted helper-image, load census, and application I/O
work. The user has also reopened native qualification; the prior investigation
pause is no longer an execution restriction.

## Implemented and reviewed scope

The helper executable is materialized in an owned child so unrelated host forks
cannot inherit its writer. Linux pristine tests pass; the unchanged-hook
parent-writer mutation fails with the expected `ETXTBSY` 26. The mutation changes
only writer ownership and keeps the actual-open observation seam and regression
identical. Sources, raw logs, and mutation bodies are retained in
`linux-image-green/` and `linux-image-mutation/`.

The fork fixture now uses `/usr/bin/true`, present on the tested Mac, instead of
missing `/bin/true`. The original gate failure and focused passing result are
in `regressions/`. That failure was a fixture-path error, not evidence of a
production regression. Protocol EOF fixtures now tolerate nonblocking pending
reads while retaining strict truncated-frame rejection and a finite deadline;
a retained peer descriptor explicitly exercises delayed closure. See
`protocol-eof-fixture/` and the independent review.

The load collector stops periodic sampling after acknowledged final cleanup,
then still requires completion and exit zero. Independent review additionally
found that a trial could omit the final cleanup census entirely and pass. It now
requires `closed` before success. The new test failed before this change and
passes afterward. Two older success fixtures were updated to emit that real
protocol checkpoint; their CPU-unavailable and pipe-cleanup assertions remain.

Five new application tests cover provider/protection errors, shared quota
pressure, ordered restoration, and rejected stale-checkpoint deletion. They pass
in the macOS gate's application suite. These assertions do not replace strict
coverage measurement or full release workload proof.

Independent reviews:

- [DDD](../../reviews/resumed-ddd.md)
- [Design](../../reviews/resumed-design.md)
- [Correctness](../../reviews/resumed-correctness.md)
- [Protocol and Linux mutation evidence](../../reviews/resumed-protocol-eof-independent.md)

## Execution records

`macos-gate/` preserves the failure in two outdated success fixtures after the
new cleanup requirement. `macos-gate-final/` preserves the later protocol-test
pending-read failure. Both are failed attempts, not accepted gates.
`macos-gate-accepted/` and `linux-gate/` are the subsequent source-bound full-gate
locations; only their completed metadata can establish a pass. Gate execution
includes scoped performance, not the complete release matrix or soak.

`native-seed201/` reruns the historical seed on the combined native patch as it
stood at that point. It exits with SIGSEGV (-11), with unchanged source
inventory; the allocator correction alone did not resolve the deferred native
failure. That failure has since been diagnosed and corrected: PAGE decoding now
rejects a header capacity a native page cannot address. See
[the diagnosis](../../reviews/terminal-page-capacity-admission.md) and
`../page-capacity/` for the red/green native proof, the passing seed, the corpus
range, and the gate. No completed native corpus beyond that recorded range or
release acceptance follows from mechanical gates.

## Remaining acceptance

Complete all 11 outstanding case groups with five full trials, fresh 12-hour
Linux soak and resource-stability assessment, physical stack/control-memory
attribution, strict whole-inventory 100% coverage, native failure resolution and
qualification, and reconciliation of all 67 ADR rows. The latest historical
coverage is 92.86% Rust lines; helper/native/platform gaps remain explicit.
[Memory review](../../reviews/resumed-memory-attribution.md) explains why existing
aggregate snapshots cannot prove the 4 KiB target or physical reader-stack cost.
