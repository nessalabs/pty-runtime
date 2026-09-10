# Loop 2 organization and design-pattern review

Reviewed 2026-09-08 on the uncommitted loop 2 tree. Scope: raw runtime facade,
application orchestration, identity/commands/replay, repository and process
adapters, native terminal adapter and bridge/build integration. Source was
changing concurrently; findings refer to the inspected paths and symbols and
must be rechecked after fixes. This review does not declare G1 or G2 complete.

## Findings

### P2: The facade does not expose all types needed to implement its ports

`src/lib.rs:19–22` exports `ISessionRepository`, but its methods require
`Arc<SessionContext>` (`crates/application/src/runtime/repository.rs:12,16`).
`SessionContext` has no public path through the facade. A consumer depending
only on `pty-runtime` cannot name this type in a custom repository implementation
without adding an internal crate dependency. The exported terminal traits have
the same issue: their signatures require `TerminalConfig`, `TerminalError`,
`TerminalCheckpoint`, `TerminalCapabilities`, `TerminalView` and other domain
types that the facade does not export; only `TerminalSize` is exposed there.

Provide deliberate public paths for the complete port signatures, including
their nested domain models. Prefer named exports over the two wildcard exports
so adding an internal port helper cannot silently enlarge the public package.
Add an external-consumer compile fixture or example depending only on the
facade and implementing custom repository/terminal adapters. Existing in-workspace
tests that import the internal crates do not prove the single-package contract.

Status: open. This blocks calling the current adapter-injection API complete.

### P2: Production native build source escapes the size gate

`scripts/gate.py:48–52` scans only `src`/`crates` and only `.rs` extensions.
`crates/infrastructure/Cargo.toml:2` now points its actual Cargo build target at
`../../scripts/native/build.rs`. The same directory contains production bridge
code (`owner.c`, `checkpoint.c`, `view.c`, `bridge.h`) compiled by that build
script. They are real production inputs despite the `scripts` directory name.

The current files are small (build script 62 nonblank lines; C/header files
32–96), so no existing file exceeds the limit. Nevertheless, the prior
out-of-tree-source risk is now an actual coverage gap. Include Cargo custom-build
targets and native bridge inputs in the inventory. Explicitly extend the
350-nonblank-line standard to hand-written C/header source if enforcing the same
limit there; its current wording specifies Rust only. Test that an oversized
build script and native source cannot silently bypass the gate.

Status: open; gate owner notified.

### P3: Supervisor file is approaching a second responsibility boundary

`crates/infrastructure/src/process/supervisor.rs` has 329 nonblank lines. It owns
the event loop and polling, request admission, child/reader construction rollback,
the `OwnedProcess` lifecycle, signal targeting and final cleanup. Most naturally
cohere as supervision, but the single-process owner already has an independent
lifecycle and failure surface from the multi-process loop.

Before adding more behavior here, extract `OwnedProcess` and its lifecycle/
cleanup methods into a focused private module; leave request scheduling and
polling in the supervisor. Keep signal identity and child ownership together
during that split. Do not break individual methods into arbitrary helpers just
to satisfy the count. There is no present hard-size violation.

Status: growth warning; not independently a correctness blocker.

## Positive organization evidence and limits

The facade forwards use cases instead of duplicating replay/lifecycle behavior.
Process code has separate spawn, endpoints, session queues, I/O, exit-watch and
supervisor modules. Repository map mutation stays in its adapter. Registration
precedes external spawn, and rollback uses lifetime identity. RAII owns native
terminals and child cleanup rather than leaving raw handles to consumers.

The native side separates ownership/allocation, checkpoint operations, view
conversion and the independent verification formatter. Rust raw bindings remain
private. `GhosttyTerminal` carries exclusive ownership; its unsafe Send rationale
is local, and mutable operations require exclusive access. Capability reporting
honestly fences feed/resize before history completion after the native oracle
found missing history. That is an appropriate adapter capability rather than
Ghostty-specific scheduling policy embedded in core state.

The application context is currently 229 nonblank lines and combines watcher
registration, replay reservation, callbacks and status publication. It remains
reviewable for the raw slice, but projection/parking should get their own
coordinator rather than accumulating all behavior here. Application-owned Mutex,
Waker and atomic quota mechanics need the DDD specialist's assessment against
ADR 0005's synchronization rule; this organization review does not waive it.

All inventoried Rust files under `src`/`crates` remain below 350 nonblank lines.
The largest inspected file is the supervisor; terminal conversion and state
files are 128–152 lines and command validation is 133. No new generic service
locator, catch-all repository, trait-per-helper layer or raw native public API
was found. Tests remain focused by subsystem rather than bloating entry modules.

## Proof assessment and final gate

Source inspection shows meaningful raw facade tests for gaps/completion,
detachment, transient input and admission; process contract/pressure tests use
real children and exercise interruption, partial writes, failure and cleanup.
Native tests include an independent full-state formatter oracle and corruption/
bounds checks. These are stronger than asserting the implementation's own
returned flags. They remain separate paths: a raw PTY test and a standalone
terminal test do not prove integrated parser ordering, authoritative replies or
parking. The complete stress/performance/soak requirements remain unproved.

This specialist inspected the source inventory and test bodies, and did not
rerun the full runtime/native suites while their authors were changing the tree.
Reported passing test counts must be sourced from the loop's preserved command
output. After the P2 fixes, re-review the actual facade and gate, run the full
gate on the final tree, and preserve its evidence before commit. Native build
pin verification and cross-target compiler selection also require the behavioral
reviewer's build reproducibility assessment; source archive validation alone
does not prove the provenance of a separately cached compiled archive.

## Re-review of loop 2 fixes

Re-inspected current source on 2026-09-08 after the facade, gate and supervisor
changes. The following dispositions supersede the open statuses above:

- **Facade P2 resolved by source inspection.** `src/lib.rs` now explicitly
  exports `ports::SessionContext` with the repository trait, all process-port
  contracts by name, both terminal traits by name, and the complete engine-neutral
  `terminal` module. Required signature models now have usable facade paths.
  There are no wildcard port exports. An external custom repository/terminal
  compile fixture remains desirable regression proof; this re-review did not
  execute one and does not claim one exists.
- **Native size coverage P2 resolved in implementation.** The gate recursively
  scans `src`, `crates`, `scripts/native` and `tests`, including `.rs`, `.c` and
  `.h`. The actual Cargo build script and production bridge are now covered.
  The expanded scan ran successfully. Two small follow-ups remain: describe
  C/header enforcement in the coding standards, whose wording still specifies
  Rust; and add negative tests for oversized native/build-script inputs. The
  eleven existing gate tests cover dependency validation, not this size branch.
- **Supervisor P3 resolved by meaningful decomposition.** The event loop is
  now 104 nonblank lines; single-process lifecycle is 124, registration/rollback
  91, signal control 27, and spawner/admission ownership 127. The split follows
  lifetime and execution responsibilities. `PendingChild` and `Admission`
  transfer through the spawn queue and release through RAII. This is not an
  arbitrary file-size split. Correctness under concurrent shutdown still belongs
  to the adversarial behavioral suite.

Session policies/status/completion now live in `domain::session`; the application
options module re-exports those definitions instead of maintaining a second
model. Domain transition methods preserve existing exit/drain facts. This is an
improvement in ownership organization; the DDD specialist must still assess the
remaining synchronization and aggregate-invariant requirements.

The mandatory gate now invokes `scripts/native/bootstrap.py` before all-feature
native Clippy/tests. The bootstrap delegates to the pinned experiment build
path, avoiding a second native dependency resolver. This fixes the visible
clean-checkout ordering omission. It is source-level verification: no fresh
empty-cache build or clean remote CI run was executed by this re-review.

Executed locally: the expanded `architecture()` check passed, and
`python3 -m unittest discover -s scripts/tests -v` passed all eleven tests.
No new blocking organization finding was identified in these changes. Keep
the full gate evidence and remaining ADR proof gaps separate from these focused
review results.
