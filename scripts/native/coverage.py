#!/usr/bin/env python3
"""Measure all four own C bridge files using real native Rust contract tests.

Rust and the pinned Zig archive remain uninstrumented. This makes no whole-project
coverage claim. The supplied source tree is copied before compilation; neither
its Cargo target nor its native build/cache is changed.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shlex
import shutil
import subprocess
import sys

BRIDGE = ("owner", "checkpoint", "view", "verification")
TESTS = ("terminal_contract", "terminal_bounds", "terminal_live_restore")
ROOT = Path(__file__).resolve().parents[2]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-root", type=Path, default=ROOT)
    parser.add_argument("--native-source", type=Path)
    parser.add_argument("--guardian-image", type=Path, required=True)
    parser.add_argument("--allocator-test", type=Path,
                        help="Optional updated allocator test copied into the frozen snapshot")
    parser.add_argument("--extra-test", type=Path, action="append", default=[],
                        help="Additional infrastructure native test copied into the snapshot")
    parser.add_argument("--boundary-tests-dir", type=Path,
                        help="Optional synthetic boundary contracts, separate from real-engine tests")
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--require-complete", action="store_true",
                        help="Fail unless all own-C lines/functions/regions/branch sides are covered")
    args = parser.parse_args()
    source_root = args.source_root.resolve()
    native = (args.native_source or source_root / "work/experiment-cache/ghostty-82232ecde55405559dec29c5466cb9e39938cb41").resolve()
    work, evidence = args.work.resolve(), args.evidence.resolve()
    if work.exists():
        raise SystemExit("Use a fresh --work directory; prior profiles must not be merged silently")
    work.mkdir(parents=True)
    evidence.mkdir(parents=True, exist_ok=False)
    records = []

    def run(name, command, *, cwd=source_root, env=None, required=True):
        process = subprocess.run(list(map(str, command)), cwd=cwd, env=env,
                                 capture_output=True, text=True)
        (evidence / f"{name}.stdout.txt").write_text(process.stdout)
        (evidence / f"{name}.stderr.txt").write_text(process.stderr)
        records.append({"name": name, "command": list(map(str, command)),
                        "cwd": str(cwd), "exit_code": process.returncode})
        (evidence / "commands.json").write_text(json.dumps(records, indent=2) + "\n")
        print(f"{name}: exit {process.returncode}", flush=True)
        if required and process.returncode:
            raise SystemExit(f"{name} failed; raw evidence preserved")
        return process

    compiler = subprocess.check_output(["xcrun", "--find", "clang"], text=True).strip()
    cov = subprocess.check_output(["xcrun", "--find", "llvm-cov"], text=True).strip()
    profdata = subprocess.check_output(["xcrun", "--find", "llvm-profdata"], text=True).strip()
    for name, command in [("clang-version", [compiler, "--version"]),
                          ("llvm-cov-version", [cov, "--version"]),
                          ("llvm-profdata-version", [profdata, "--version"]),
                          ("rustc-version", ["rustc", "-Vv"])]:
        run(name, command)

    snapshot = work / "source"
    snapshot.mkdir()
    for name in ("Cargo.toml", "Cargo.lock", "AGENTS.md", "coding_standards.md"):
        shutil.copy2(source_root / name, snapshot / name)
    ignore = shutil.ignore_patterns("target", "__pycache__", "*.profraw")
    for folder in ("src", "crates", "helpers", "scripts", "tests", "examples"):
        shutil.copytree(source_root / folder, snapshot / folder, ignore=ignore)
    if args.allocator_test:
        shutil.copy2(args.allocator_test.resolve(), snapshot / "scripts/native/tests/allocator-contract.c")
    if args.boundary_tests_dir:
        for name in ("boundary-contract.c", "boundary-shim.c", "boundary-faults.h",
                     "run_boundary_contract.py"):
            shutil.copy2(args.boundary_tests_dir / name, snapshot / "scripts/native/tests" / name)
    tests = list(TESTS)
    for test in args.extra_test:
        shutil.copy2(test.resolve(), snapshot / "crates/infrastructure/tests" / test.name)
        tests.append(test.stem)
    # Keep verification's source/archive/library contract intact in a private copy.
    native_copy = work / "native-cache" / native.name
    shutil.copytree(native, native_copy, ignore=shutil.ignore_patterns(
        ".zig-cache", "zig-out", "zig-cache", ".git"))
    archive = native.parent / (native.name + ".tar.gz")
    shutil.copy2(archive, native_copy.parent / archive.name)
    library = native_copy / "zig-out/lib/libghostty-vt.a"
    library.parent.mkdir(parents=True)
    shutil.copy2(native / "zig-out/lib/libghostty-vt.a", library)
    image = work / "pty-runtime-guardian-image"
    shutil.copy2(args.guardian_image.resolve(), image)
    run("verify-native", [sys.executable, snapshot / "scripts/native/verify_source.py",
                          native_copy, "--built"], cwd=snapshot)

    wrappers = work / "bin"
    wrappers.mkdir()
    wrapper = wrappers / "cc"
    # Append -O0 after build.rs's -O2 so executable regions remain interpretable.
    wrapper.write_text("#!/bin/sh\nexec " + shlex.quote(compiler)
                       + ' "$@" -O0 -fprofile-instr-generate -fcoverage-mapping -fprofile-update=atomic\n')
    wrapper.chmod(0o755)
    profiles = work / "profiles"
    profiles.mkdir()
    overrides = {
        "PATH": str(wrappers) + os.pathsep + os.environ["PATH"],
        "CARGO_TARGET_DIR": str(work / "target"),
        "PTY_RUNTIME_GHOSTTY_SOURCE": str(native_copy),
        "PTY_RUNTIME_GUARDIAN_IMAGE": str(image),
        "LLVM_PROFILE_FILE": str(profiles / "build-%p-%m.profraw"),
        "RUSTFLAGS": "", "CARGO_ENCODED_RUSTFLAGS": "",
        "CC": str(wrapper),
    }
    env = dict(os.environ, **overrides)
    manifest = {str(p.relative_to(snapshot)): sha(p) for p in snapshot.rglob("*") if p.is_file()}
    (evidence / "source-manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    identity = {
        "platform": platform.platform(), "source_root": str(source_root),
        "snapshot": str(snapshot), "environment_overrides": overrides,
        "native_archive_sha256": sha(archive), "native_library_sha256": sha(library),
        "native_build": json.loads((native_copy / "experiment-build.json").read_text()),
        "guardian_image_sha256": sha(image), "cc_wrapper": wrapper.read_text(),
        "coverage_driver_sha256": sha(Path(__file__).resolve()),
        "denominator": [f"scripts/native/{name}.c" for name in BRIDGE],
        "scope": "Own C bridge only; Rust and pinned native engine are uninstrumented",
    }
    (evidence / "identity.json").write_text(json.dumps(identity, indent=2) + "\n")
    command = ["cargo", "test", "--locked", "--no-run", "--message-format=json",
               "-p", "pty-runtime-infrastructure", "--features", "ghostty"]
    for test in tests:
        command += ["--test", test]
    built = run("build-contracts", command, cwd=snapshot, env=env)
    binaries = []
    for line in built.stdout.splitlines():
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        if message.get("executable") and message.get("target", {}).get("name") in tests:
            binaries.append((message["target"]["name"], message["executable"]))
    if {name for name, _ in binaries} != set(tests):
        raise SystemExit("Missing expected contract executable")
    outcomes = []
    for name, binary in binaries:
        env["LLVM_PROFILE_FILE"] = str(profiles / f"run-{name}-%p-%m.profraw")
        result = run(name, [binary, "--nocapture", "--test-threads=1"], cwd=snapshot, env=env, required=False)
        outcomes.append({"test": name, "exit_code": result.returncode, "binary_sha256": sha(Path(binary))})
    if args.allocator_test:
        binary = work / "allocator-contract"
        command = [wrapper, "-std=c11", "-Wall", "-Wextra", "-Werror", "-g",
                   "-I", native_copy / "include", snapshot / "scripts/native/tests/allocator-contract.c",
                   snapshot / "scripts/native/owner.c", library, "-o", binary]
        run("build-allocator-contract", command, cwd=snapshot, env=env)
        env["LLVM_PROFILE_FILE"] = str(profiles / "run-allocator-%p-%m.profraw")
        result = run("allocator-contract", [binary], cwd=snapshot, env=env, required=False)
        binaries.append(("allocator-contract", str(binary)))
        outcomes.append({"test": "allocator-contract", "exit_code": result.returncode, "binary_sha256": sha(binary)})
    if args.boundary_tests_dir:
        # The runner redirects only separately compiled own-C objects. Its shim
        # calls the real library; synthetic cases do not establish engine behavior.
        boundary_dir = work / "boundary"
        run("build-boundary-contract", [sys.executable,
            snapshot / "scripts/native/tests/run_boundary_contract.py",
            "--build-only", boundary_dir], cwd=snapshot, env=env)
        binary = boundary_dir / "boundary-contract"
        env["LLVM_PROFILE_FILE"] = str(profiles / "run-boundary-%p-%m.profraw")
        result = run("boundary-contract", [binary], cwd=snapshot, env=env, required=False)
        binaries.append(("boundary-contract", str(binary)))
        outcomes.append({"test": "boundary-contract", "exit_code": result.returncode,
                         "binary_sha256": sha(binary), "synthetic_faults": True})
    (evidence / "test-outcomes.json").write_text(json.dumps(outcomes, indent=2) + "\n")
    raw = sorted(profiles.glob("run-*.profraw"))
    if not raw:
        raise SystemExit("No runtime profiles; no coverage result may be claimed")
    merged = work / "bridge.profdata"
    run("merge-profiles", [profdata, "merge", "-sparse", *raw, "-o", merged])
    objects = [binaries[0][1]]
    for _, binary in binaries[1:]:
        objects += ["-object", binary]
    covered_files = [str(snapshot / "scripts/native" / f"{name}.c") for name in BRIDGE]
    common = [*objects, f"-instr-profile={merged}", *covered_files]
    run("coverage-report", [cov, "report", *common])
    exported = run("coverage-export", [cov, "export", *common])
    exported_json = json.loads(exported.stdout)
    measured = {Path(file["filename"]).name for file in exported_json["data"][0]["files"]}
    if measured != {name + ".c" for name in BRIDGE}:
        raise SystemExit(f"Incomplete C denominator: {measured}")
    run("coverage-annotated", [cov, "show", "-show-branches=count", *common])
    print("Coverage reported for all four bridge files; inspect failures and missing paths.", flush=True)
    if any(outcome["exit_code"] for outcome in outcomes):
        raise SystemExit("A contract failed; coverage is partial evidence, not acceptance")
    if args.require_complete:
        totals = exported_json["data"][0]["totals"]
        missing = [metric for metric in ("lines", "functions", "regions", "branches")
                   if not totals[metric]["count"]
                   or totals[metric]["covered"] != totals[metric]["count"]]
        if missing:
            raise SystemExit("Own-C 100% target not met: " + ", ".join(missing))


if __name__ == "__main__":
    main()
