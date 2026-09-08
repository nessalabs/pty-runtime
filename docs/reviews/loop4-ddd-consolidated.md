# Independent loop-4 DDD and boundary-contract consolidation

Reviewed 2026-09-08 against the current dirty worktree based on
`52ad04c3519616e6cedea9ac8707406970a40ed7`. The reviewer authored no loop-4
production behavior; prior work added independent tests and instrumentation
evidence. This report consolidates existing scoped architectural reviews with
fresh inspection of their actual current boundaries. It is not the independent
organization review or a substitute for behavioral acceptance.

**Verdict: no confirmed unresolved P1/P2 dependency-direction, domain ownership,
or typed-boundary defect in the inspected scope.** Native behavioral qualification,
full coverage, final gate and release workload proof remain pending. In particular,
the currently investigated corpus case 132 checkpoint `EngineFailure` does not
become a passed behavior merely because its failure is mapped to a portable type.

## Source identity and verification

[Source manifest](../verification/loop4/ddd-consolidated/source-manifest.json)
SHA-256: `a198b56dbdd11a41d5644b882dfe6403638f3af1596b30d68026b67f6e9e293a`.
[Identity](../verification/loop4/ddd-consolidated/identity.json) records capture
time and HEAD. The manifest includes facade, domain/application/infrastructure,
helper, native/guardian build sources and manifests. It identifies the reviewed
snapshot; concurrent native work is not retroactively covered.

Independently executed `cargo metadata --locked --no-deps --format-version 1`
and asserted zero domain dependencies and exactly the domain dependency in
application. [Recorded edges](../verification/loop4/ddd-consolidated/dependency-edges.json)
confirm OS, crypto and optional pinned event-stream dependencies remain in
infrastructure. Both core crate roots forbid unsafe code. Source inspection found
no native/OS/event serialization implementation imported into core behavior.

The six independently added tests and focused Clippy runs are recorded in
[the acceptance-test audit](independent-acceptance-tests.md). That evidence
supports specific contracts, not a full DDD proof or a coverage percentage.
The coordinator owns `python3 scripts/gate.py` after final source changes; its
earlier attempt 2 is not a final run for later tests/native changes.

## Current boundary assessment

| Area and concrete source | Assessment |
| --- | --- |
| Facade `src/runtime.rs`, core manifests and `application/{process,terminal,scheduling}` ports | Facade composes concrete infrastructure; application owns use cases and I-prefixed replaceable ports. Domain owns validated sizes/lifetimes/ordering/admission policy. `std::time::Instant` timing stays application/adapter observation rather than becoming domain transition authority. Diagnostic process ID is an opaque scalar; core does not signal or discover OS identities. |
| `infrastructure/terminal/mod.rs:13,50`, `state.rs:91`, `scripts/native/{owner.c,checkpoint.c,verify_source.py,build.rs}` | Native pointers, allocator ABI, Ghostty result values and decoder details stay infrastructure. Core sees opaque checkpoint bytes plus compatibility/byte/control metadata and typed errors/progress. Restore checks compatibility before native decoder construction. The current compatibility string includes patch SHA `d3cee6c6548ab641d4f528a5ce6280424a1caece5f67a4d48074f0c48f1b0af4`, matching the verifier's declared patch identity. The verifier validates allowed native input hashes and the built archive's patch/library stamp; build uses the uniquely named static library. |
| `scripts/native/owner.c:9` | The actual pinned Zig log2-alignment ABI is handled in the C adapter: invalid shifts/budget are rejected; allocation above malloc alignment uses `posix_memalign`; requested-byte accounting/free remain local. No allocator enum/pointer or physical-memory policy enters domain. Native requested bytes remain distinct from allocation overhead/RSS. The earlier reject-all-exponents-above-4 experiment was superseded because legitimate compression needs higher alignment. |
| `domain/terminal/checkpoint.rs`, `application/projection/completion.rs:158`, `stream_end.rs:22` | READY means usable active state, not source validation or complete history. Applied versus skipped history has explicit portable outcomes and cumulative accounting. The application retains restore memory/ciphertext until finished source validation; native pages do not become replay bytes. Successful End waits for drain, applied prefix, no resize/reply, and finished restoration. A failure cannot be silently promoted into successful complete history. |
| `domain/projection/transfer.rs`, `application/projection/{snapshot.rs:24,journal.rs,transfer.rs}` | Domain owns lifetime/sequence/retained-range/End policy; application owns finite payloads, observers, tickets and quota leases. Byte offsets and ordered mutation sequence remain different types. A transfer pairs the checkpoint and continuation at the serialized descriptor boundary; returned payload/view pins expose borrowing and keep their reservations. Event-stream offsets are absent from this ordering policy. |
| `domain/checkpoint/cleanup.rs`, `infrastructure/checkpoint/{file.rs:49,arena.rs,inventory.rs,cleanup.rs,filesystem.rs}` | Domain owns finite limits and a portable observed-result report. Names, permissions, descriptors, flock and iterator errors remain infrastructure. Constructors reject incomplete scan or cleanup failure rather than admit indefinitely behind a live prefix. Deletion is namespace-scoped, anchored and nonrecursive; live owners retain locks. Same-UID hostile name replacement is an explicit trust limitation, not a promised inode-conditional unlink or cross-owner restore. |
| `infrastructure/process/{guardian.rs:183,registration.rs:13,lifecycle.rs:48}`, `helpers/guardian/src` | Helper wire/version/generation/topology, discovery and signaling authority remain infrastructure/helper concerns. Guardian retains direct S ownership and channels across registration failures. Only actual W wait facts become `ExitStatus`; S/G death, EOF and kill acknowledgment are distinct from W completion. Reader/drain and supervision failure are separate application events. Infrastructure does not expose helper lifecycle enums as domain completion. |
| `application/diagnostics`, `runtime/context.rs`, `infrastructure/process/lifecycle.rs` | Fixed unlabeled counters/timings observe existing boundaries without deciding admission or lifecycle. Logical reservations and caller-retained gauges are not claimed as RSS. Concrete shared diagnostics need no speculative external-service port. `Timing` distinguishes success, abandoned failure and unavailable replacement instrumentation. Adapter-held diagnostics Arc does not retain session event/context ownership. |
| `infrastructure/event_stream/{publisher.rs:94,codec.rs,status.rs}`, facade optional export | Optional pinned transport and serialization remain infrastructure. Caller supplies sink/incarnation/executor; construction creates no implicit persistence or task. A bounded Source is retained before fallible encoding, Prepared identity survives cancellation/error, and only a matching receipt with increasing store offset acknowledges the PTY cursor. Foreign event cursor/schema/record types are exposed only through the explicitly optional adapter, not portable core models. |

## Consolidated findings disposition

No new severity-bearing architecture finding is confirmed. Previously recorded
findings are not duplicated or marked resolved solely on an author's assertion:

* [Guardian architecture](guardian-architecture.md): anchor transport allocation
  now precedes fork, retaining the post-fork child ledger; successor descriptor
  inventory failure retires its actual group rather than silently retaining EOF
  endpoints. Current architecture and [correctness review](guardian-correctness.md)
  preserve the direct-child/membership authority boundary and qualification limits.
* [Checkpoint crash cleanup](checkpoint-crash-cleanup-independent.md): current
  constructor rejects incomplete enumeration as well as identified abandoned
  remainder. The live-prefix starvation P2 remains resolved in inspected source
  and its existing deterministic regression.
* [Ordered transfer](ordered-transfer-organization.md), [correctness](ordered-transfer-correctness.md)
  and [READY review](ready-restoration-correctness.md): inspected immutable End,
  wake ownership, serialized snapshot boundary and source-validation ownership
  agree with the recorded fixes. New cancelled parked-read test adds the late
  completion/close ownership combination without altering production semantics.
* [Event-stream architecture](event-stream-architecture.md): receipt offset
  comparison remains strictly increasing before updating acknowledgment or
  clearing pending state (`publisher.rs:119,128`). The prior P2 remains resolved.
* [Diagnostics architecture](runtime-diagnostics-architecture.md): ownership
  gauges are separate from activity and reset; the poisoned-context release and
  untimed replacement tests complement inspected adapter/context separation.

These dispositions do not close the older guardian production design's resource,
fault-injection, signed packaging or supported-target qualification requirements.

## Pending behavior and required re-review

The terminal reviewer is investigating a real checkpoint failure in corpus case
132 after 131 distinct cases reportedly passed. Those counts are coordinator
status, not independently rerun corpus evidence here. Native cursor/pending-wrap
and row-wrap correctness remain a behavioral acceptance gate. A typed
`EngineFailure` is an appropriate boundary representation; it does not establish
that failure on valid supported state is acceptable. The domain must not adopt
Ghostty-specific row/page mutation algorithms to hide that engine defect.

Any further native patch must update the exact patch/source/build stamp and
compatibility identity together, preserve failures, rebuild the library, and rerun
the public/core and real-native contracts against that identity. The earlier C
ASan/UBSan record used an older explicitly hashed native patch; it is not final
instrumentation evidence for this changing candidate. Native assertions/aborts
remain process failures; Rust panic containment does not isolate an abort or
instrument the ReleaseFast Zig engine.

Re-review is required for changed files at these boundaries, especially changed
restoration semantics/capabilities, compatibility rules, helper status authority,
source/permit lifetimes or external acknowledgment policy. An engine-only fix
that preserves the inspected ports does not invalidate inward dependency
direction; its behavior and exact compatibility still require qualification.

Full measured coverage is incomplete and no percentage is asserted. Unexecuted
resource/discovery/startup failure seams, final current-source macOS/Linux gate,
platform support, real workload/resource sweeps, repeated performance cases,
10,000 lifecycle cycles, 100,000 attach/detach operations and the 12-hour mixed
soak remain tracked in the acceptance audit/ADR ledger. This DDD verdict does not
authorize calling the overall release ready while those required proofs or the
native behavioral issue remain unfinished.
