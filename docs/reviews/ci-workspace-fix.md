# Experiment workspace CI fix review

Reviewed 2026-09-08 against base `1c5d17e`, before committing the fix. Scope:
`experiments/pty/Cargo.toml` and the two added fixture checks in
`scripts/gate.py`. This is the organization specialist's review; reported remote
job outcomes are not independently verified by this source review.

**Disposition: no actionable organization or correctness defect found in this
fix.** The explicit empty `[workspace]` makes the independently runnable fixture
its own workspace root and preserves its separate lockfile, edition, dependency
pins and release profile. It does not add the experiment allocator or workload
helpers to the production library. The root gate now calls fixture formatting
and locked Clippy explicitly, matching the separate experiment CI checks;
production `--workspace` commands alone cannot cover this nested workspace.

Executed locally:

- `cargo metadata --manifest-path experiments/pty/Cargo.toml --locked --no-deps
  --format-version 1`: succeeds; identifies only `pty-experiment-harness` as a
  workspace member, with workspace root `experiments/pty` and its own target
  directory.
- `cargo fmt --manifest-path experiments/pty/Cargo.toml --check`: passes.
- `cargo clippy --manifest-path experiments/pty/Cargo.toml --locked --all-targets
  -- -D warnings`: passes.

`git diff --name-only -- experiments` identifies only the fixture manifest as
changed. No workload source, protocol, dependency lockfile, raw measurement, or
historical baseline was changed by this fix at review time. The source manifest
in `experiments/run.py:228` includes Cargo manifests; therefore subsequent runs
will correctly have a different source hash. Retain historical source hashes
with their original measurements. Do not rewrite them to make an older baseline
appear to have used this manifest. This review makes no new performance claim.

The full repository gate and remote experiment rerun remain the implementation
loop's final verification obligations. Fixture Clippy proves build/lint coverage,
not successful execution of native measurements or integrated runtime behavior.
