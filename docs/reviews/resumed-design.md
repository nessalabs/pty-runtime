# Resumed independent organization and design review

Reviewed current working-tree source on 2026-09-08/09, based on HEAD `46dbec64384e4519e751086ef47feb5bac11024d`. Applied AGENTS.md and coding_standards.md. Scope is all uncommitted implementation/test changes: helper image materialization, constructor seam and fixtures, projection I/O tests and terminal fault controls, and release-load final census. Historical evidence directories and earlier review conclusions were consulted only as context. No builds, tests or gate executed by this reviewer; the coordinating review loop owns execution and acceptance.

## Finding (resolved in re-review below)

**P2 — Trial success does not require the final cleanup census.** `scripts/release/load.py:109` accepts successful process exit plus a complete marker, and `:125` requires only the two measurement checkpoints before `:134` records `passed=True`. The final resource assertions exist only inside the optional `phase == 'closed'` branch at `:96-98`. A fixture that emits measurement_start, measurement_end, complete and exits zero before the periodic deadline is therefore accepted without any final descendant/zombie observation. This is a pre-existing protocol-validation hole, not a regression introduced by the new sampling guard. It is directly relevant to accepting this final-census review scope: the driver now explicitly treats closed as the terminal sampling transition but does not require reaching it for success.

Require an acknowledged closed census before emitting trial_result, and add a deterministic real-child omission test with the periodic deadline held in the future. Preserve the current rejection of nonzero exit, missing complete, descendants and zombies. The existing disappearance-before-closed test instead forces a periodic census exception; it does not exercise a clean completion that omits closed. Static path analysis establishes this finding; no failing execution result is claimed here. The prior final-census review deliberately retained a trusted-fixture protocol assumption; this review challenges that assumption at the release evidence acceptance boundary.

## Organization assessment

No P1/P2 organization blocker was found in the helper change. `HelperImage` keeps directory/path ownership and bundled-image validation. The narrow `image_materialize` module owns mask/fork/write/reap as a cohesive infrastructure operation, with an ordinary concrete internal API and portable ProcessError translation. A process-writing port or generic subprocess service would add indirection without a replacement boundary. Its private success/error exit categories are compact and symmetric; named constants could aid a larger protocol but are not a necessary abstraction here.

The parent creates immutable path/byte inputs and does not own an executable writer. The child owns the writer until close or process exit; the parent retains the image/path cleanup owner until the child is reaped. Mask restoration occurs before waiting, and a restoration error does not bypass ownership cleanup. Raw child operations remain segregated from parent allocation/error handling. The documented unbounded filesystem/child wait and host reaping/atfork requirements must remain visible in API claims; source inspection does not prove those calls bounded or bypass host atfork handlers.

The test hook's parent-PID branch and RefCell are additional complexity, but serve a concrete mutation-testing requirement: both the original parent-writer defect and the replacement child writer can trigger the same actual-open observation without relocating the seam. The PID check prevents executing a Rust observer in the child. Socket ownership/handshake belongs in the separate test-only module, callbacks and unwind handling stay in the parent, and reaping precedes panic propagation. Removing this seam or introducing an interface in production would weaken evidence or expand runtime API without a demonstrated benefit. Ordinary non-test builds compile out hook behavior.

The constructor test owns the unrelated pending launch through BlockedFork, releases it before judging the expected failing exec, and retains a kill/reap guard for spawned children. This isolates the writer-inheritance contract from generic process fixture helpers. Separate filesystem/mask tests are appropriately focused; they do not claim every OS error path or every platform has been executed.

Projection test changes remain in the application test boundary. FaultProtector decorates the existing port fixture, terminal probe fields alter test-provider outputs, and the two focused modules cover admission/cleanup pressure versus rejected parking provider results. They assert retained bytes and resources across failure/retry/close rather than merely counting calls. Repeated resource assertions are small enough that creating a new generic assertion framework would obscure each scenario's contract; no production abstraction is warranted by these tests.

The final-census guard is a small, correctly placed change: snapshot/assert/ack failures still unwind into failure cleanup, while queued complete output after a validated closed census no longer triggers a periodic sample. The remaining P2 above concerns the success precondition, not that guard. A full protocol state-machine extraction is unnecessary to fix it.

All scoped Rust modules and fixtures remain below the repository size threshold. No native, application or domain dependency was introduced by image materialization. This source review does not certify performance, coverage, cross-platform behavior, the full release matrix or soak readiness. The P2 requires a retained regression and independent re-review before the expanded final-census scope is accepted.

## Inspected source identity

| File | Nonblank lines | SHA-256 |
| --- | ---: | --- |
| `crates/infrastructure/src/process/image.rs` | 87 | `505fc327cc1583cda99d165705fe7efadafb2eb1f1b2bea48480ee6ccd3a3ac6` |
| `crates/infrastructure/src/process/image_materialize.rs` | 187 | `2f3d02bcc547feabed0d65398abe34ccbd37381ae6c71696cfa75e0e02917604` |
| `crates/infrastructure/src/process/image_materialize_hook.rs` | 72 | `6b882c754d990d04891cc391c1a92473964d1283ad1b042d0049791794499ce7` |
| `crates/infrastructure/tests/fixtures/process_image_fork.rs` | 165 | `a41846b814763e33a2bc7bb9edc6315cd418586d9b6bb616073b4b3149a94f81` |
| `crates/infrastructure/tests/fixtures/process_image_materialize.rs` | 102 | `4d3afffec148af414720ce4d648f377cbe2665fb5ede161831f224adaca9412a` |
| `crates/application/src/projection/tests/io_parking_faults.rs` | 192 | `ed6b5bda904faad26e230b2cdcf19efa8f8ad0eb5efe8e6a150ac4f32d68808e` |
| `crates/application/src/projection/tests/io_pressure.rs` | 217 | `f75329484869756833376d93b017e16c29409ca34032f3ae6178c52dcc905a30` |
| `crates/application/src/projection/tests/terminal.rs` | 226 | `39afa1777b8f4db5322c8f0e4138d6369020904a249ec1fb1a8564c5e0726ae0` |
| `scripts/release/load.py` | 210 | `9176921b0e505773b53ad767a01ee559ef2ad099d5f9116905d1bc612c9be5ec` |
| `scripts/tests/test_load_final_census.py` | 96 | `9adde746c886d1afbabc2e040da503e117409067236b521185b62e478dcfb713` |

## Fix re-review

Independently inspected the revised source and retained `work/resumed-validation/missing-closed-red.log`, `missing-closed-green.log`, `image-fork-green.log` and original `gate.log`. The P2 is **resolved**: `scripts/release/load.py:110` now requires closed in checkpoints after successful exit/complete validation and before any successful trial_result. The earlier closed branch samples, validates descendants/zombies and acknowledges; failures escape the try block, so dictionary membership cannot let a failed closed checkpoint reach this success assertion. This is the minimal cohesive correction and adds no unnecessary protocol abstraction.

The added `test_clean_exit_without_closed_census_is_rejected` keeps the periodic deadline in the future with `advance_clock=False`, receives both measurement checkpoints and complete, and asserts rejection without trial_result. It therefore reaches the previously unguarded success path instead of failing accidentally in periodic sampling. Read RED reports only that new assertion failing because passed was True; read GREEN reports all five focused tests passing. This independently observed source/evidence supports closure of the finding; this reviewer did not execute the commands.

The constructor fixture correction at `crates/infrastructure/tests/fixtures/process_image_fork.rs:52` changes only the unrelated child's eventual executable from `/bin/true` to `/usr/bin/true`. Read-only filesystem inspection independently confirmed the latter exists and the former does not on this macOS host. The original gate log fails at blocker release/reap with NotFound, consistent with a fixture target-path error after the fork handshake. The revised focused log passes the constructor regression. The actual-open timing, unrelated pending fork, image execution assertion and owned cleanup guards are unchanged, so the correction preserves the writer-inheritance test contract. It is not evidence of another runtime defect and does not replace the original Linux ETXTBSY RED. A full gate rerun and required Linux validation remain coordinator responsibilities.

No unresolved P1/P2 design or organization finding remains in this inspected scope. Full release acceptance is not claimed.

### Superseding source identities for fixes

| File | SHA-256 |
| --- | --- |
| `scripts/release/load.py` | `582bc49aeaec304662168ebbfc08c1b1123201a4580348c4bbb506a5562fa939` |
| `scripts/tests/test_load_final_census.py` | `9adde746c886d1afbabc2e040da503e117409067236b521185b62e478dcfb713` |
| `crates/infrastructure/tests/fixtures/process_image_fork.rs` | `eb15591f80c400332ce9ea1f78fad4834aa02cb4e21f34ef739ccdf0ae515e9c` |
