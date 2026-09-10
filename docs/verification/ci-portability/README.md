# CI native CPU portability fix

This narrowly scoped review loop addresses Runtime gate run
[34211241576](https://github.com/nessalabs/pty-runtime/actions/runs/34211241576),
Ubuntu job 102012497082 on base commit
`06508816021b17f325cb35dce626bef56bc15e20`.

The failed log is retained in `original-ubuntu-failure.log`. It records SIGILL
in the actual `terminal_contract` executable after a cache hit. The same run's
uncached Ubuntu MSRV job and macOS gate passed. The source cache defect is proven;
the exact faulting instruction was not captured, so its attribution to unsupported
cached CPU instructions remains strongly supported rather than conclusively localized.
See `../../archive/reviews/native-cache-portability.md` for the evidence and limits.

The fix makes the native build explicitly target the baseline CPU, includes
build options and driver identity in its stamp, and changes both workflow cache
keys to exclude legacy host-specific archives. It does not skip any tests or
change terminal behavior. The full gate also runs new tests that exercise the
actual build-driver command/cache path with compiler/download calls substituted.

## Executed validation

`python3 scripts/record_validation.py --output docs/verification/ci-portability/macos-gate -- python3 scripts/gate.py`
passed on macOS 26.6 arm64 in 32.29 seconds. `macos-gate/metadata.json` binds the
complete unchanged source inventory, base commit, compiler, platform, command and
result; `macos-gate/command.log` contains the entire gate, including the scoped
runtime performance check. Existing pinned baseline native cache was reused.
This is a real local gate execution, not a fresh Linux native build claim.

Independent behavioral and organization/DDD review reports are in
`../../archive/reviews/ci-portability-independent.md` and
`../../archive/reviews/ci-portability-organization.md`. GitHub execution of this fix must
be inspected after push before reporting the Ubuntu job repaired. The original
failed run remains failed and is not relabeled as a pass.

## GitHub result after push

Commit `52ad04c3519616e6cedea9ac8707406970a40ed7` was pushed to private `main`.
[Runtime run 34243560775](https://github.com/nessalabs/pty-runtime/actions/runs/34243560775)
completed successfully: Ubuntu 24.04 gate, macOS 15 gate and Rust 1.85 MSRV.
[Experiment run 34243560737](https://github.com/nessalabs/pty-runtime/actions/runs/34243560737)
also passed on Ubuntu and macOS. Full logs and authoritative job metadata are
retained beside this report with the `github-*-52ad04c` names. These executions
validate the narrow CPU/cache fix at that commit, not the uncommitted runtime
implementation or its outstanding full release qualification. They do not recover
the original job's missing faulting instruction.
