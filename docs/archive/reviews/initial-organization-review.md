# Initial organization and design-pattern review

Date: 2026-09-08. Scope: ADRs 0001–0005 and the existing Rust experiment
source. This is the initial specialist review requested by the owner. It is
design guidance and a source inventory, not a passing implementation review.
The runtime source did not exist when this review began; later source must be
reviewed on its own revision.

## Findings from the inspected source

| Finding | Evidence | Required disposition |
| --- | --- | --- |
| Production boundaries are specified but not implemented | ADR 0005 requires Cargo-enforced domain/application/infrastructure direction; the only inspected manifest was `experiments/pty/Cargo.toml`, a standalone harness | Establish internal crates before production use cases; verify resolved Cargo edges instead of checking directory names |
| The harness combines unrelated responsibilities | `experiments/pty/src/main.rs` was 635 lines: allocator accounting at line 19, measurement at 55–101, Unix PTY/I/O at 102–170, reader ownership at 203–343, workload fixtures at 367–598, and CLI/reporting at 599 | Preserve existing evidence, but do not copy this entry point into the library; split future changes along these responsibilities before growing it further |
| The concurrent fixture is difficult to audit independently | `concurrent` at 469–598 combines child launch/barriers, probe timing, reaping, result decoding, assertions, cleanup, and JSON assembly | Extract named stages with typed results when this fixture changes; retain the workload/timing contract and compare resulting measurements |
| Platform separation is useful but incomplete as a production boundary | `platform/linux.rs` was 105 lines, `platform/macos.rs` 131, and `platform/mod.rs` 23; common PTY operations and raw Unix calls remain in `main.rs` | Keep platform modules, but place all production native ownership and OS error translation in the process adapter |
| Experiment assertions are not a library error policy | `pair`, `write_all_fd`, and `main` panic on errors/invalid arguments; allocator instrumentation is global | Do not reuse them as production implementations. The library needs typed partial-I/O errors and rollback ownership, and must not impose its allocator on its host |

These are organization findings, not claims that the historical measurements
are invalid. No runtime test, native integration test, or stress test ran as part
of this documentation review. No critical production defect can be proved from
production source that was not yet present.

## Proposed crate and module boundaries

Follow the direction required by [ADR 0005](../adr/0005-domain-boundaries-and-adapters.md).
Use one public package and a small number of private crates; create modules as
behavior is implemented rather than adding empty files for this entire list.

| Crate | Focused module candidates | Owns and excludes |
| --- | --- | --- |
| `pty-runtime-domain` | `identity`, `command`, `session`, `replay`, `admission`, `terminal`, `checkpoint`, `error` | Concrete values and state-transition rules; no native handles, serialization DTOs, clocks that read the OS, threads, channels, or implementation imports |
| `pty-runtime-application` | `ports`, `spawn`, `observe`, `input`, `resize`, `cancel`, `shutdown`, `projection`, `parking`, `restore` | Use cases, I-prefixed boundary contracts, translated outcomes; no Ghostty, libc, filesystem, crypto, executor, or event-stream implementation |
| `pty-runtime-infrastructure` | `process/unix`, `process/linux`, `process/macos`, `process/reader`, `process/writer`, `process/supervisor`, `repository`, `terminal/ghostty`, `checkpoint/file`, `checkpoint/protector`, `events`, `clock` | Concrete external adapters, synchronization/scheduling and boundary conversion; policy decisions stay in application/domain |
| `pty-runtime` | `runtime`, `session`, `attachment`, `options`, `lib` | Public API, composition and default adapters; no second implementation of lifecycle, replay, or parking rules |

A separate private `ghostty-sys` crate is reasonable if required to isolate
pinned native build/link instructions and raw bindings. Its API must be used
only by the safe terminal adapter. It is not a second public engine API.
Additional infrastructure crates are justified when they isolate native build
requirements or a substantial optional dependency, not merely to shorten files.

Keep `lib.rs` and `mod.rs` primarily declarations and deliberate exports. Avoid
unrestricted `pub use ...::*` that accidentally expands the public API. Error
modules may be split by subsystem when they represent different contracts;
avoid a generic string error that erases partial-write or partial-resize outcomes.

The process adapter needs explicit internal boundaries between reader, ordered
writer and shared exit supervision. One dedicated reader per admitted live PTY
is the accepted baseline; parking must not silently stop it. Do not turn that
choice into a permanent waiter thread per session. Shared supervision and its
OS mechanism need their own reviewed ownership and integration proof.

Keep a single owner for each native terminal. Parking and restoration use
generation-checked orchestration around that owner rather than sharing a raw
pointer among unrelated workers. Separate the encoder's synchronous native
ownership interval from slow store commits. A model lock must not also become
the runtime registry lock or the cancellation path.

## Size and complexity review rules

Line counts are review triggers, not correctness proofs or reasons to slice a
cohesive function arbitrarily. The observed 635-line harness is problematic
because it contains at least six responsibilities, not simply because it crosses
a numeric threshold. Apply these initial thresholds and revise them only with a
recorded rationale and reviewer acceptance:

1. A hand-written production Rust file above 400 physical lines, a function above
   80 lines, or nesting deeper than four control-flow levels requires an explicit
   organization review before merge. Count comments and tests in the file total;
   report test lines separately so test coverage is not discouraged.
2. Any file with multiple independently changing owners or multiple external
   boundary types requires review even below those thresholds. Examples include
   terminal parsing plus encryption, or registry admission plus OS reaping.
3. Review all touched unsafe functions and callbacks regardless of length. Each
   needs documented pointer/buffer lifetimes, aliasing, thread ownership, error
   behavior, and an observable cleanup path. Generated raw bindings may receive
   a documented line-count exemption, never an exemption from adapter review.
4. A threshold exception must identify the path, current size, coherent
   responsibility, why splitting would impair review, responsible reviewer, and
   next recheck trigger. Do not silence checks globally or move code into an
   `include!` file solely to evade the rule.
5. Treat parameter lists as a cohesion signal: seven or more independent
   parameters require review. Use a typed command/configuration only when its
   fields form a meaningful concept; do not hide arbitrary services in a bag.

For each loop, inventory owned `.rs` files with `rg --files` while excluding
`target`, vendored source, generated bindings, and `work`. Report exceptions
explicitly; tests and experiment source remain reviewable and cannot be
silently excluded because they are large. Automated checks should identify
candidate functions using parsed Rust syntax, or clearly label a simpler scan
as approximate; a regex match is not a cyclomatic-complexity proof.

## Patterns to use and patterns to reject

- Use a session aggregate for invariant-preserving transitions and concrete
  values for identity, lifetime, cursor and generation. Reject an anemic domain
  whose fields are mutated directly by every adapter, and reject one giant
  aggregate that owns OS handles, stores, threads and every byte buffer.
- Use ports at meaningful external seams and adapters for foreign types. Reject
  a trait for every small helper, per-byte dynamic dispatch, a generic service
  locator, or core code that downcasts a terminal trait to Ghostty.
- Use RAII for acquired descriptors, children, permits and native allocations,
  with explicit shutdown/completion for observable outcomes. Reject relying on
  `Drop` alone to perform bounded graceful shutdown or report real exit status.
- Use explicit admitted-operation/state transitions for cancellation and
  generation tokens for asynchronous park/restore completions. Reject boolean
  flag combinations that admit impossible states or stale completion callbacks
  that can revive a removed session.
- Use one ordered stream for user input and authoritative replies. Reject direct
  FFI-callback writes, callback reentrancy into model mutation, and independent
  observer-generated replies.
- Keep replay eviction, lossless parser staging, snapshot pins and input budgets
  distinct. Reject a catch-all unbounded queue or a shared buffer whose eviction
  silently drops authoritative parser input.
- Prefer cohesive concrete implementations first. Shared-reader strategies are
  experiment comparators, not a reason to build unqualified dynamic handoff,
  an abstract scheduling framework, or multiple competing ownership models.

## Recurring review gate

Every implementation/review loop records revision, changed paths, commands,
specialist findings and dispositions. A green unit test result does not replace
this review. The organization specialist must answer the following questions:

- [ ] Do resolved Cargo dependencies preserve core independence, including
  enabled optional features and target-specific edges? Can domain/application
  build and test without the native runtime?
- [ ] Does every changed module have one stated responsibility? Are entry files
  and the facade free of duplicated policy, broad exports and implementation
  accumulation? Are threshold exceptions recorded and reviewed?
- [ ] Are foreign values translated inside adapters? Do public/core types avoid
  native pointers, OS errors, serialization schemas and dependency cursors?
- [ ] Are lifecycle transitions centralized and separate from exit/drain,
  observer attachment and terminal residency? Are partial outcomes explicit?
- [ ] Can a reviewer trace acquisition, transfer and cleanup of every new child,
  descriptor, reader, terminal, operation permit and checkpoint pin? Do failure
  and abandonment branches release ownership exactly once?
- [ ] Are locks and execution boundaries documented, with no global lock across
  PTY I/O, native encoding, storage, event publication or waits? Is cancellation
  independent of saturated data paths?
- [ ] Does each new abstraction eliminate a demonstrated coupling or model a
  real contract? Are implementations swappable without downcasts or conditional
  policy hidden inside domain code?
- [ ] Do deterministic tests exercise meaningful failure/race behavior through
  ports, and do separately identified real-adapter tests prove the integration?
  Are fixture helpers distinct from production behavior and free of circular
  expected-result calculations?
- [ ] Are pending ADR claims marked unverified until matching executed evidence
  exists? Do reports retain failures/timeouts and name exact platform coverage?

Block the loop on a forbidden dependency edge, unsound ownership, duplicated
authoritative behavior, unbounded new resource, or unresolved high-severity
finding. A mere size trigger requests examination; it is not independently a
defect. Record acceptance and rationale for lower-severity exceptions instead
of claiming there were no findings. Re-run focused tests after fixes, then run
the repository gate on the final tree before committing the loop.

## Foundation review before first push

Reviewed the new root and internal manifests, four `lib.rs` stubs,
`coding_standards.md`, `AGENTS.md`, `scripts/gate.py`, README and both workflows
on 2026-09-08. These were uncommitted at inspection, so there is no reviewed
commit ID yet. This review supersedes the earlier absence-of-workspace finding:
the four layers now exist and their declared dependency direction is correct.
All production files are small; none contains runtime behavior to certify.

The adopted coding standard's **350 nonblank production Rust lines** is the
authoritative mechanical limit. It supersedes the initial 400-physical-line
proposal above and does not permit size waivers. The other complexity/cohesion
signals remain specialist-review questions. The historical fixture is a
documented future refactor; preserving its protocol and existing evidence is
required when splitting it.

### Findings and disposition

| Severity | Finding and evidence | Required action / status |
| --- | --- | --- |
| P2 | `scripts/gate.py:19–25` checks a named package only if metadata happens to contain that name. It does not assert expected core package presence or workspace membership, and it checks no infrastructure/facade direction. A renamed or removed core member could evade the purported architecture gate. | Assert required package identities/membership and allowed internal edges for all layers. Keep external infrastructure dependencies allowed. Open; do not describe the gate as proving the entire architecture yet. |
| P2 | `Cargo.toml` advertises Rust 1.85, but `.github/workflows/runtime.yml:17–18` only installs stable and runs the generic gate. Installing a toolchain does not explicitly select it, and stable does not prove the minimum version. | Select the intended toolchain explicitly. Add an MSRV build/test job or record MSRV as unqualified until executed; this does not prove the stubs currently fail on 1.85. Open qualification gap. |
| P3 | `scripts/gate.py:26–30` scans only `src` and `crates`; a future production Cargo target outside those folders would avoid its 350-line check. | Keep this layout as an explicit restriction or derive production target roots from Cargo metadata. No current bypassing target exists. Open forward-looking gate-hardening item. |

No P0/P1 organization defect is present in the reviewed stubs. The facade's
single module re-export is deliberate and does not yet introduce broad exports
of an implemented API. Keeping the historical experiment outside the production
workspace is appropriate: it prevents its allocator and native fixture helpers
from becoming a library dependency. The repository instructions correctly make
three specialist reviews mandatory and distinguish mechanical checks from ADR
proof. The workflow uses read-only repository permissions and separate runtime
and measurement jobs.

This review inspected source; it did not run the repository gate and cannot
substitute for its recorded result. After the gate findings are changed, inspect
the actual fixes, update their dispositions here, and rerun the final gate.

## Re-review: foundation fixes and initial replay domain

Re-inspected the uncommitted foundation on 2026-09-08 after fixes, including
`scripts/gate.py`, runtime workflow, Cargo target metadata, and the new domain
identity/replay implementation and tests.

- **Resolved — package gate completeness.** The gate now requires all four
  named packages at their intended manifests, checks workspace membership,
  checks allowed core dependencies and internal dependency direction, and
  validates that internal dependency paths resolve to the local package. This
  addresses the concrete missing/renamed-package bypass and reversed-edge
  finding. It does not prove semantic source-layer purity or every possible
  future dependency configuration; specialist review remains required.
- **Resolved — toolchain selection.** The runtime workflow now explicitly sets
  `RUSTUP_TOOLCHAIN: stable`. The separate MSRV qualification gap remains open;
  stable execution is not Rust 1.85 proof. This foundation is not a release.
- **Verified for current scope — size coverage.** Cargo metadata reports only
  `src/lib.rs` and the three `crates/*/src/lib.rs` target roots. All current
  production source is within the gate's recursive `src`/`crates` scan. The
  future out-of-tree-target concern remains a checklist item, not a present
  bypass or reason to reject this foundation.

The new domain organization is cohesive: `identity.rs` defines identity and
cursor values, `replay.rs` owns byte retention and cursor rules, and
`replay_tests.rs` provides a focused colocated test module. Nonblank counts are
42, 162 and 87 respectively; `lib.rs` is six lines of declarations/exports.
There are no unnecessary traits, external types, OS calls, service locators or
unsafe blocks. The buffer maintains its invariant through methods rather than
public mutable fields. Typed `Gap`, `Bytes` and `Pending` outcomes keep replay
distinct from process completion. Payload Debug output is deliberately redacted.

No new actionable organization/design-pattern defect was found in this slice.
Resource reservation is explicitly delegated to the future application layer;
the `VecDeque` and returned `Vec` allocations are **not** evidence of integrated
global memory admission. Likewise, public lifetime construction states a
non-reuse obligation that the eventual issuing owner must enforce. Review both
contracts again when the orchestration is added.

Executed focused checks in the repository: `python3 -c 'import scripts.gate as
g; g.architecture()'` passed; `cargo test --locked -p pty-runtime-domain` passed
five tests with zero failures. Source inspection confirms the independent
reference retains the complete produced stream and compares its suffix rather
than copying the deque implementation. The current domain test module does not
yet test SessionId validation and is not process/runtime/native evidence.
The parent implementation loop must still preserve the full final gate output.
