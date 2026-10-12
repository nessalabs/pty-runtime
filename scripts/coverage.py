#!/usr/bin/env python3
"""Measure Rust workspace/helper coverage and require 100% lines/functions/regions.

This is a readiness gate in addition to gate.py. Native C, other platforms, and
branch instrumentation need their own evidence; this command cannot qualify them.
Run native/bootstrap.py first. Requires cargo-llvm-cov, llvm-tools-preview, and
the reviewed coverage compiler wrapper on Darwin for continuous fork counters.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import tempfile
import time

from record_validation import ROOT, source_manifest


def clean_environment():
    environment = os.environ.copy()
    for name in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "LLVM_PROFILE_FILE",
                 "CARGO_LLVM_COV", "CARGO_LLVM_COV_SHOW_ENV",
                 "CARGO_LLVM_COV_TARGET_DIR", "CARGO_TARGET_DIR",
                 "PTY_RUNTIME_GUARDIAN_IMAGE", "LLVM_PROFILE_FILE_NAME"):
        environment.pop(name, None)
    return environment


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    before = source_manifest()
    started = time.time()
    environment = clean_environment()
    (ROOT / "work").mkdir(exist_ok=True)
    build_root = Path(tempfile.mkdtemp(prefix="coverage-run-", dir=ROOT / "work"))
    phases = []

    def run(name, command, env=environment):
        print(f"+ {name}: {' '.join(command)}", flush=True)
        phase_start = time.time()
        with (output / f"{name}.log").open("w") as log:
            try:
                code = subprocess.run(command, cwd=ROOT, env=env,
                                      stdout=log, stderr=subprocess.STDOUT).returncode
            except OSError as error:
                log.write(f"could not start command: {type(error).__name__}\n")
                code = 127
        phases.append({"name": name, "command": command, "exit_code": code,
                       "elapsed_seconds": time.time() - phase_start})
        return code == 0

    run("rustc-version", ["rustc", "-Vv"])
    run("coverage-version", ["cargo", "llvm-cov", "--version"])
    thresholds = ["--fail-under-lines", "100", "--fail-under-functions", "100",
                  "--fail-under-regions", "100", "--fail-uncovered-lines", "0",
                  "--fail-uncovered-functions", "0", "--fail-uncovered-regions", "0"]
    helper_target = build_root / "uninstrumented-helper"
    helper_ok = run("prepare-helper", [
        "cargo", "build", "--locked", "--release", "--manifest-path",
        "helpers/guardian/Cargo.toml", "--target-dir", str(helper_target),
        "--bin", "pty-runtime-guardian"])
    helper = helper_target / "release/pty-runtime-guardian"
    helper_hash = hashlib.sha256(helper.read_bytes()).hexdigest() if helper_ok else None
    workspace_env = {**environment, "PTY_RUNTIME_GUARDIAN_IMAGE": str(helper),
                     "CARGO_TARGET_DIR": str(build_root / "workspace")}
    # Forked materialization exits with _exit. The already verified Darwin
    # continuous mode keeps its shared, atomic counters without exit hooks.
    if platform.system() == "Darwin":
        workspace_flags = ["-C", "llvm-args=-instrprof-atomic-counter-update-all"]
        for section in ("cnts", "bits", "data"):
            workspace_flags += ["-C", f"link-arg=-Wl,-sectalign,__DATA,__llvm_prf_{section},{os.sysconf('SC_PAGE_SIZE'):#x}"]
        workspace_env.update(RUSTFLAGS=" ".join(workspace_flags),
                             LLVM_PROFILE_FILE_NAME="workspace-%p%c.profraw")
    helper_env = {**environment, "CARGO_TARGET_DIR": str(build_root / "helper")}
    if helper_ok and run("workspace-clean", ["cargo", "llvm-cov", "clean", "--workspace"], workspace_env):
        # LLVM startup changes an instrumented helper's environment before exec.
        # Preserve the exact Empty-policy assertion by measuring the helper in a
        # separate run. The override is an existing production composition seam.
        common = ["cargo", "llvm-cov", "--locked", "--workspace", "--all-targets",
                  "--no-fail-fast", "--no-report"]
        run("workspace-all-features", common + ["--all-features"], workspace_env)
        # --no-report already preserves counters between matrix phases in the
        # pinned local tool; combining it with --no-clean is rejected.
        run("workspace-raw", common + ["--no-default-features"], workspace_env)
        run("workspace-event-stream", common + ["--no-default-features", "--features",
                                                "event-stream"], workspace_env)
        run("workspace-report", [
            "cargo", "llvm-cov", "report", "--json", "--output-path",
            str(output / "workspace.json")] + thresholds, workspace_env)
    # Helper paths end through fork/_exit/group kill. Continuous counters retain
    # those executions without changing shutdown or the workload environment policy.
    helper_evidence = output / "helper-continuous"
    run("helper-continuous", [sys.executable, "scripts/guardian/coverage.py",
        "--work", str(build_root / "helper-continuous"), "--evidence", str(helper_evidence),
        "--require-complete"], helper_env)
    helper_export = helper_evidence / "export.stdout.txt"
    if helper_export.exists():
        (output / "helper.json").write_bytes(helper_export.read_bytes())
    after = source_manifest()
    metadata = {
        "platform": platform.platform(), "architecture": platform.machine(),
        "started_unix": started, "elapsed_seconds": time.time() - started,
        "source_files": before, "sources_unchanged": before == after,
        "uninstrumented_helper_sha256": helper_hash, "phases": phases,
        "run_owned_build_and_profile_root": str(build_root),
        "passed": before == after and all(p["exit_code"] == 0 for p in phases),
        "scope": "Rust workspace feature matrix and separate helper units, startup and protocol fixtures",
        "continuous_workspace_profile": platform.system() == "Darwin",
        "not_measured_by_this_command": [
            "native C bridge", "upstream native dependency", "other platform configurations",
            "Python/build tooling", "Rust branch/MC/DC coverage"],
        "qualification": "This scoped result cannot establish whole-project readiness.",
    }
    (output / "metadata.json").write_text(json.dumps(metadata, indent=2, sort_keys=True) + "\n")
    if before != after:
        (output / "source_files_after.json").write_text(json.dumps(after, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"passed": metadata["passed"], "sources_unchanged": before == after,
                      "output": str(output)}))
    return 0 if metadata["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
