# Instrumented launch environment diagnosis

Independent scoped diagnosis on macOS arm64, 2026-09-08. Canonical production was
read-only throughout. Final changes are confined to two test files. Exact source
and uninstrumented-helper fingerprints: `docs/verification/coverage-fixture-diagnosis/source.json`.

## Reproduction and actual cause

The frozen coverage mirror's full run stopped at the exact-empty Rust fixture
with workload exit 91. Reproduced with:

```sh
cargo llvm-cov test --no-clean --test raw_child_contract --locked --all-features
```

`reproduced-red.log` retains that original failure. Scratch-only boolean probes
(`diagnostic-red.log`) showed only the environment predicate failed: all three
terminal descriptors, controlling terminal, canonical cwd, literal argument,
PATH removal and synthetic override checks passed. Exactly one extra key existed;
`LLVM_PROFILE_FILE` was absent. Key-only diagnostics identified
`__LLVM_PROFILE_RT_INIT_ONCE` (`diagnostic-keys-red.log`). No environment values
were printed. Direct `env -i` execution of the instrumented fixture independently
created that same key without using the PTY adapter (`direct-instrumented-red.log`).

The first proposed fixture adjustment was deliberately tested against a strict
uninstrumented `/usr/bin/env` child. That test also failed: the helper executable
had inherited the coverage compiler flags, and its own coverage startup inserted
this key before it launched the workload. `boundary-keys-red.log` retains the
exact two observed key names. Therefore merely allowing the key in the Rust
fixture would conceal instrumentation-induced pollution at the exec boundary.
That proposal was not accepted on its own.

## Narrow resolution

The root-workspace coverage run uses the existing supported
`PTY_RUNTIME_GUARDIAN_IMAGE` override with an independently built **uninstrumented**
helper. No production environment exception is added. Helper coverage must be
collected in a separately identified helper run; it cannot be credited from this
uninstrumented image. Scratch preparation/proof:

```sh
env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS -u LLVM_PROFILE_FILE -u CARGO_LLVM_COV \
  cargo build --locked --release --manifest-path helpers/guardian/Cargo.toml \
  --target-dir target/guardian-uninstrumented --bin pty-runtime-guardian
PTY_RUNTIME_GUARDIAN_IMAGE="$PWD/target/guardian-uninstrumented/release/pty-runtime-guardian" \
  cargo llvm-cov test --no-clean --test raw_child_contract --locked --all-features
```

The exact external boundary test then passes, proving only the synthetic override
reaches `/usr/bin/env`. The instrumented Rust fixture still creates its own marker
at startup, so `tests/fixtures/contract_support.rs` allows that one named key in
its post-startup check, with the reason documented. It still verifies cwd, literal
arguments, all TTY properties, PATH removal, override and actual resize behavior.

`tests/raw_child_contract.rs` adds
`empty_environment_is_exact_at_uninstrumented_exec_boundary`: the public runtime
launches `/usr/bin/env` with Empty, the same removal/override sequence, and requires
exactly the sole synthetic line (allowing only PTY LF/CRLF translation). Unexpected
output fails without printing potentially inherited values. A scratch mutation
changing that probe to Inherit fails (`negative-inherit-red.log`). There is no
prefix-based allowlist, coverage skip, or production environment workaround.

## Verification and scope

All three final tests pass both with the instrumented workspace plus uninstrumented
helper (`final-instrumented.log`) and in the canonical uninstrumented build
(`canonical-uninstrumented.log`). The earlier successful intermediate proof is
also retained in `uninstrumented-helper-instrumented-host.log`. All abbreviated
log paths above are under `docs/verification/coverage-fixture-diagnosis`.

Generated the requested exploratory JSON at
`work/coverage-baseline/work/coverage.json` using `cargo llvm-cov report --json
--output-path work/coverage.json`. It contains partial data from the failed full
run and diagnostic reruns; it is not a complete baseline or coverage acceptance.
Its zero branch counts mean branches were unmeasured, not fully covered. Later
coverage collection must bind source, flags, measured components and platform to
its report. Root owns the fresh full baseline, separate helper coverage and gate.
