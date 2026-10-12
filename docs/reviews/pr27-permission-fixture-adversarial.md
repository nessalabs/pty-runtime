# PR27 deterministic permission fixture: adversarial review

Independent review of the test-only EACCES correction. The reviewer did not author this correction. Outcome: no unresolved P1/P2 in the fixture delta. Targeted execution and the completed mechanical gate are independently verified.

## Findings and resolution

The old test assumed a directory with mode 0500 must reject creation. That is not a valid oracle for privileged Unix users. The corrected test describes its narrower contract honestly: materializer recovery from an open operation returning EACCES, rather than enforcement of operating-system permission policy. Production implementation and fault mechanism are unchanged.

The test calls the existing scalar `Point::Open` seam with result -1, errno EACCES, and one permitted consumption. Static call-path review confirms Open is consumed only in `child_write -> open_exclusive`, after the real fork. The child writes its F receipt and sets its own errno, returns status 3, and exits through the actual raw child-exit path. The parent restores its signal mask, waits for the real owned child, and maps that status to PermissionDenied. No parent-side fake Result replaces this path.

`consumed == b"F"` is exact, so neither an unconsumed seam nor multiple attempts can pass. The test also asserts the exact PermissionDenied result, complete caller signal-mask membership, no created destination, and unchanged existing sibling bytes. Its subsequent ordinary `write` call has no injected fault and must create the same destination with exact retry bytes while preserving the mask. Thus leaving the fault armed, returning an unrelated error, creating a partial file, modifying a sibling, or leaking the full child mask is observable. Existing RAII cleanup restores the original mask and removes the private randomized directory on unwind.

The inherited receipt fd is collected only after the child is reaped. Its existing write-direction shutdown avoids waiting for unrelated forked copies of the descriptor; the correction introduces no TLS, allocation, locks, or callbacks after fork.

A validation issue was found during review: `pr27-eacces-targeted01/02` used a nonmatching module filter. In particular targeted02 reports zero tests and 46 filtered, despite a successful command exit. Those runs are retained but are not acceptance evidence. The corrected targeted03 filter runs exactly the named new test and reports one pass, zero failures, and 45 filtered. This closes the validation gap for the scoped fixture.

## Evidence

`work/pr25-blockers/pr27-eacces-targeted03/metadata.json` records `cargo test --locked -p pty-runtime-infrastructure --all-features --lib materializer_open_eacces`, exit zero, unchanged sources, and the platform/toolchain. Its source bindings match all three files below. The actual command log names `process::image::materialize::tests::materializer_open_eacces_preserves_existing_files_and_caller_mask`; this review checked the test count rather than relying only on metadata's passed flag.

| File | SHA-256 |
| --- | --- |
| `crates/infrastructure/tests/fixtures/process_image_materialize.rs` | `36dbdc0ce4a6b8b85ca2b2584ba380d70d6c441a6cd3c0b6727ba280feb3edb0` |
| `crates/infrastructure/tests/fixtures/process_image_syscall_fault.rs` | `4111e4835a2999fa50e937d37cebbfae135c55929a9ed6d094be4406047683b6` |
| `crates/infrastructure/src/process/image_materialize.rs` | `7749a7a2155710d4d9cfa95753fc3300f425129fc82d5c6db5508a36d5277ddd` |
| `work/pr25-blockers/pr27-eacces-targeted03/command.log` | `9ed8fce29e68c5c927a3b123e844e173634802fb9c331ac9e8fe0a4b590422e4` |

No builds or test processes were launched by this reviewer. This correction does not establish OS discretionary-access-control enforcement, a completed hosted Linux/MSRV run, full performance qualification, or 100% coverage. The completed local full-gate evidence is recorded below.

## Related stale-decision documentation correction

Independently reviewed the coordinating parent's one-paragraph edit to the Current decision at the top of `direct-counter-tool-adversarial.md`. The historical body is unchanged. The former top-level claim that the V10 identity initializer remains blocked contradicted its later exact-expression re-review, which explicitly closes that oracle failure, and the later V12 review closes the concrete missing-forwarded-parent finding. The replacement accurately points to those scoped closures and the corrected-stack integrity assessment while retaining failed strict 100% coverage and incomplete release qualification. It also distinguishes the subsequently recorded user merge authorization from readiness evidence. No historical failure, retained measurement, caveat, or scope qualification was removed. No P1/P2 finding in this documentation correction.

## Completed mechanical gate evidence

Independently read `work/pr25-blockers/pr27-eacces-gate01/metadata.json` and its actual command log. `python3 scripts/gate.py` exits zero in 67.51088380813599 seconds, with unchanged source identity; all 361 recorded source hashes independently match current files. The command log records the exact new EACCES test passing twice in its executed feature checks and ends with the mechanical-gate success statement. Log SHA-256: `1e729b2fb3c2106e59bdc0ab569bdfaf36efacbe1fa830192e232fce10b2c4a1`.

Scoped adversarial result remains PASS with no unresolved P1/P2. Hosted CI and unchanged strict coverage limitations remain separate requirements; this local gate does not supersede them.
