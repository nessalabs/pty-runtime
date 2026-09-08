#!/usr/bin/env python3
"""Measure unchanged guardian source via units and actual protocol fixtures on Darwin.

Continuous counters are page aligned and atomic, so fork/_exit/group-SIGKILL
need no modified process cleanup. This is helper-only, not workspace coverage.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-root", type=Path, default=ROOT)
    parser.add_argument("--fork-profile-hook", type=Path,
                        help="Test-only atfork profiler hook; never linked into ordinary helpers")
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    args = parser.parse_args()
    if platform.system() != "Darwin":
        raise SystemExit("This page-aligned continuous-profile method was validated only on Darwin")
    source, work, evidence = args.source_root.resolve(), args.work.resolve(), args.evidence.resolve()
    work.mkdir(parents=True, exist_ok=False)
    evidence.mkdir(parents=True, exist_ok=False)
    records = []

    def run(name, cmd, env=None, required=True):
        result = subprocess.run(list(map(str, cmd)), cwd=work, env=env, text=True, capture_output=True)
        (evidence / (name + ".stdout.txt")).write_text(result.stdout)
        (evidence / (name + ".stderr.txt")).write_text(result.stderr)
        records.append({"name": name, "command": list(map(str, cmd)), "cwd": str(work),
                        "exit_code": result.returncode})
        (evidence / "commands.json").write_text(json.dumps(records, indent=2) + "\n")
        print(f"{name}: exit {result.returncode}", flush=True)
        if required and result.returncode:
            raise SystemExit(f"{name} failed; evidence retained")
        return result

    snapshot = work / "source"
    helper = snapshot / "helpers/guardian"
    shutil.copytree(source / "helpers/guardian", helper,
                    ignore=shutil.ignore_patterns("target", "*.profraw"))
    shutil.copytree(source / "scripts/guardian", snapshot / "scripts/guardian",
                    ignore=shutil.ignore_patterns("__pycache__"))
    manifest = {str(p.relative_to(snapshot)): sha(p) for p in snapshot.rglob("*") if p.is_file()}
    (evidence / "source-manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    rust = run("rustc-version", ["rustc", "-Vv"]).stdout
    sysroot = run("rust-sysroot", ["rustc", "--print", "sysroot"]).stdout.strip()
    host = next(line.split(": ", 1)[1] for line in rust.splitlines() if line.startswith("host: "))
    llvm = Path(sysroot) / "lib/rustlib" / host / "bin"
    run("llvm-cov-version", [llvm / "llvm-cov", "--version"])
    run("llvm-profdata-version", [llvm / "llvm-profdata", "--version"])
    pagesize = os.sysconf("SC_PAGE_SIZE")
    profiles = work / "profiles"
    profiles.mkdir()
    # A compile-time runtime filename works with the actual helper's empty env.
    filename = work / "filename.c"
    filename.write_text("const char __llvm_profile_filename[] = "
                        + json.dumps(str(profiles / "helper-%p%c.profraw")) + ";\n"
                        + "const char profile_directory[] = " + json.dumps(str(profiles)) + ";\n")
    filename_object = work / "filename.o"
    run("compile-filename", ["clang", "-c", filename, "-o", filename_object])
    flags = ["-Z", "coverage-options=branch", "-C", "instrument-coverage", "-C", "llvm-args=-instrprof-atomic-counter-update-all",
             "-C", "link-arg=" + str(filename_object)]
    hook = None
    if args.fork_profile_hook:
        hook = work / "atfork.c"
        shutil.copy2(args.fork_profile_hook.resolve(), hook)
        hook_object = work / "atfork.o"
        run("compile-atfork", ["clang", "-c", hook, "-o", hook_object])
        flags += ["-C", "link-arg=" + str(hook_object)]
    for section in ("cnts", "bits", "data"):
        flags += ["-C", f"link-arg=-Wl,-sectalign,__DATA,__llvm_prf_{section},{pagesize:#x}"]
    env = dict(os.environ, CARGO_TARGET_DIR=str(work / "target"), RUSTFLAGS="",
               CARGO_ENCODED_RUSTFLAGS="\x1f".join(flags))
    # Do not inherit a profile override which would disable the linked %c path.
    env.pop("LLVM_PROFILE_FILE", None)
    identity = {"platform": platform.platform(), "source_root": str(source), "page_size": pagesize,
                "compiler_flags": flags, "filename_source": filename.read_text(),
                "scope": "All compiled guardian src/*.rs plus shared protocol.rs; Linux discovery explicitly unmeasured",
                "driver_sha256": sha(Path(__file__).resolve()),
                "production_source_modified": False, "atfork_hook_sha256": sha(hook) if hook else None}
    (evidence / "identity.json").write_text(json.dumps(identity, indent=2) + "\n")
    cargo = ["cargo", "--locked", "--manifest-path", str(helper / "Cargo.toml")]
    # Cargo's subcommand precedes its manifest/lock options.
    run("build-helper", [cargo[0], "build", *cargo[1:], "--bins"], env)
    built = run("build-units", [cargo[0], "test", *cargo[1:], "--bin", "pty-runtime-guardian",
                                "--no-run", "--message-format=json"], env)
    unit = next(json.loads(line)["executable"] for line in built.stdout.splitlines()
                if line.startswith("{") and json.loads(line).get("executable"))
    binary = work / "target/debug/pty-runtime-guardian"
    # Build-script execution is not part of the runtime test population.
    for profile in profiles.glob("*.profraw"):
        profile.unlink()
    results = [run("units", [unit, "--test-threads=1"], env, required=False)]
    results.append(run("actual-protocol", [sys.executable, snapshot / "scripts/guardian/probe.py",
                                          "--helper", binary], env, required=False))
    raw = sorted(profiles.glob("*.profraw"))
    if not raw:
        raise SystemExit("No runtime profiles; no coverage claim permitted")
    merged = work / "guardian.profdata"
    run("merge", [llvm / "llvm-profdata", "merge", "-sparse", *raw, "-o", merged])
    files = sorted((helper / "src").glob("*.rs"))
    files.append(helper / "src/../../../scripts/guardian/protocol.rs")
    # Export both entire objects, then audit their complete source inventory.
    # LLVM normalizes source filters inconsistently for Rust #[path] modules.
    common = [binary, "-object", unit, f"-instr-profile={merged}"]
    run("report", [llvm / "llvm-cov", "report", *common])
    exported = run("export", [llvm / "llvm-cov", "export", *common])
    data = json.loads(exported.stdout)
    measured = {Path(f["filename"]).resolve() for f in data["data"][0]["files"]}
    expected = {p.resolve() for p in files}
    absent = expected - measured
    (evidence / "denominator.json").write_text(json.dumps({
        "measured": sorted(map(str, measured)), "unmeasured_platform_source": sorted(map(str, absent))
    }, indent=2) + "\n")
    if absent != {helper / "src/discovery_linux.rs"} or measured - expected:
        raise SystemExit("Unexpected helper source denominator; inspect inventory")
    run("annotated", [llvm / "llvm-cov", "show", "-show-branches=count", *common])
    saturated = [{"function": f["name"], "region": r} for f in data["data"][0]["functions"]
                 for r in f["regions"] if r[4] >= 2 ** 63 - 1]
    (evidence / "counter-sanity.json").write_text(json.dumps({
        "saturated_regions": saturated, "trustworthy": not saturated
    }, indent=2) + "\n")
    (evidence / "binary-profile-manifest.json").write_text(json.dumps({str(p): sha(p)
        for p in [binary, Path(unit), *raw]}, indent=2) + "\n")
    if saturated:
        raise SystemExit("Saturated coverage counters; report is not trustworthy")
    if any(result.returncode for result in results):
        raise SystemExit("Coverage is partial evidence because a fixture failed")


if __name__ == "__main__":
    main()
