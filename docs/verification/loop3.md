# Review loop 3: integrated projection and encrypted parking

Recorded 2026-09-08 against the exact source manifests linked below, based on
`cc69ab47aeb73d717a702edeb14540605463e743`. This is a reviewed implementation
checkpoint, not full G2/G3 or release acceptance. The ordered observer continuation,
production foreground guardian, event-stream integration and full release workloads
remain required. The [requirements ledger](requirements.md) preserves their scope.

## Implemented and exercised

The Rust facade now composes actual Ghostty, private encrypted checkpoint storage,
a shared model scheduler and a separate bounded blocking pool. Raw and projected
sessions are selected at admission. A projected session retains lossless bounded
parser staging independently of evictable raw replay. Native feed, replies and
OS/model resize follow one authoritative order. Replies are generated once even
with multiple observers. Returned views and binary checkpoint pins retain quota
reservations until dropped.

Idle parking commits encrypted immutable state before releasing native ownership.
Output racing a commit invalidates release. Restore authenticates the source and
reaches complete retained history before allowing mutation at the pinned native
revision. A parked checkpoint request reads the saved source without creating a
server-side native owner. This binary snapshot path does not yet include the
ordered byte/resize continuation required for a complete observer transfer.

Process lifecycle remains independently controllable while model/storage work is
pending. Failed and uncertain storage cleanup remains charged to the runtime.
Repeated `forget` reports cleanup failure and retains the registry entry. Shutdown
releases native state, key/store service ownership and worker resources even if old
session handles survive. Actual Unix tests verify retained closed handles release
their per-session wake descriptors. Original-group cancellation remains the default;
the standalone guardian under development is not integrated in this checkpoint.

## Executed evidence

| Validation | Evidence | Result |
| --- | --- | --- |
| Full `python3 scripts/gate.py`, macOS arm64 | [log](loop3/macos-gate/command.log), [source manifest](loop3/macos-gate/metadata.json) | Pass, sources unchanged |
| Full `python3 scripts/gate.py`, Linux x86_64 | [log](loop3/linux-gate/command.log), [source manifest](loop3/linux-gate/metadata.json) | Pass, sources unchanged; source-file manifest identical to macOS gate |
| Scoped release throughput, Linux x86_64 | [raw](loop3/linux-performance/raw.txt), [records](loop3/linux-performance/records.jsonl), [metadata](loop3/linux-performance/metadata.json) | Pass; same 1/16-session correctness workload |
| Full workspace/all-target/all-feature tests on Rust 1.85.0, macOS arm64 | [log](loop3/macos-msrv/command.log), [source manifest](loop3/macos-msrv/metadata.json) | Pass, sources unchanged |
| Default automatic parking, real child and real Ghostty | [log](loop3/default-parking/command.log), [source manifest](loop3/default-parking/metadata.json) | Pass; resident before threshold, observed parking at 60.142807208 seconds, same process alive |
| Scoped release throughput, macOS arm64 | [raw](loop3/macos-performance/raw.txt), [records](loop3/macos-performance/records.jsonl), [metadata](loop3/macos-performance/metadata.json) | Pass; 4 MiB per independent producer, exact raw byte and parser counts plus native screen suffix |

The macOS throughput trial measured 80.8 MB/s at one session and 120.6 MB/s aggregate
at sixteen sessions. The Linux trial measured 82.6 MB/s at one session and
103.7 MB/s aggregate at sixteen. These are single scoped acceptance trials on
different hosts, not a controlled platform comparison. Input release skew,
startup, cleanup, runtime budgets and point-sampled host RSS/FD/descendant counts are
in the raw records. RSS excludes separate producer/helper processes and is not peak
RSS or PSS. Both cases ended with zero descendants and five host descriptors while
the runtime object remained alive. No 128-session capacity, 60-second repetitions,
latency percentile, lifecycle-count or 12-hour soak claim follows from these trials.
Performance is now mandatory in every acceptance-loop mechanical gate; failure and
timeout evidence are retained and covered by evidence-writer tests.

The [source-under-test archive](loop3/source-under-test.tar.gz) preserves the
source files transferred for Linux qualification, including the then-unintegrated
standalone helper sources included by performance's broad identity hash. Those
helper files were not built or exercised by this runtime gate and are not accepted
as production guardian work. Runtime source manifests match across macOS and Linux.

## Independent review

- [Coordinator correctness and DDD](../reviews/projection-correctness.md): fixed
  invalid minimum budgets, panic paths that retired scheduling before cleanup,
  retained wake descriptors, and unrestricted residency assignment. Real and
  injected-adapter regressions independently rechecked the fixes.
- [Coordinator organization and ownership](../reviews/projection-coordinator-organization.md):
  independently reviewed the specialist's implementation; fixed plaintext snapshot
  under-reservation when a protector advertises compression.
- [Runtime wiring DDD and SOLID](../reviews/projection-wiring-ddd.md): independently
  reviewed the integrator's code; moved pure cross-policy validation into domain,
  preserved durable close failures despite abandoned or saturated observer waits.
- [Runtime wiring organization](../reviews/projection-wiring-organization.md):
  separately challenged composition, ownership inventory, service boundaries,
  rollback, teardown and file cohesion. No unresolved P1/P2 in reviewed scope.

The G1 child/registration contract tests and their earlier independent proof are
also included in this gate; see [G1 raw acceptance](g1-raw-acceptance.md). All ADR
milestones retain the unproven rows in the requirements ledger. External providers
and uninterruptible OS operations must eventually return; this checkpoint invents
no completion bound for those external calls.
