#!/usr/bin/env python3
"""Measure guardian continuous coverage via units and actual fixtures on Darwin.

Continuous counters are page aligned and atomic, so fork/_exit/group-SIGKILL
need no modified process cleanup. Inventory and counter checks fail closed;
--require-complete additionally requires 100% lines, functions and regions.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shlex
import shutil
import subprocess
import sys
import tarfile

import coverage_checks
from coverage_checks import (export_saturation, inventory, parse_profile,
                             profile_conservation, require, same_members)

ROOT = Path(__file__).resolve().parents[2]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def dep_sources(binary, cwd):
    depfile = binary if binary.suffix == ".d" else binary.with_suffix(".d")
    text = depfile.read_text().replace("\\\n", " ")
    rules = []
    for line in text.splitlines():
        if ": " in line and not line.startswith("#"):
            lhs, rhs = line.split(": ", 1)
            if str(binary) in shlex.split(lhs):
                rules.append({(cwd / p).resolve() for p in shlex.split(rhs)})
    require(len(rules) == 1 and rules[0], f"ambiguous source dependencies: {depfile}")
    require(all(p.suffix == ".rs" and p.is_file() for p in rules[0]),
            "missing or unexpected non-Rust dependency")
    return rules[0]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-root", type=Path, default=ROOT)
    parser.add_argument("--fork-profile-hook", type=Path,
                        help="Test-only atfork profiler hook; never linked into ordinary helpers")
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--require-complete", action="store_true")
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
    tool_paths = [Path(__file__).resolve(), Path(coverage_checks.__file__).resolve(),
                  Path(sysroot) / "bin/rustc", llvm / "llvm-cov", llvm / "llvm-profdata",
                  Path(shutil.which("clang")).resolve()]
    wrapper = os.environ.get("RUSTC_WRAPPER")
    compiler_manifest = None
    if wrapper:
        selected_wrapper = Path(wrapper).resolve(strict=True)
        compiler_manifest = selected_wrapper.parent / "frozen-candidate.json"
        require(compiler_manifest.is_file(), "coverage wrapper requires its frozen input manifest")
        frozen = json.loads(compiler_manifest.read_text())["sha256"]
        require(str(selected_wrapper) in frozen, "selected wrapper is not bound by its manifest")
        require(all(sha(Path(p)) == expected for p, expected in frozen.items()),
                "coverage compiler inputs differ from their frozen manifest")
        tool_paths += [compiler_manifest, *map(Path, frozen)]
    require(not args.require_complete or compiler_manifest is not None,
            "complete readiness requires the reviewed measurement compiler")
    require(not os.environ.get("RUSTC_WORKSPACE_WRAPPER"), "nested workspace wrapper is not verified")
    require(not (args.require_complete and args.fork_profile_hook), "readiness forbids fork profiling hooks")
    tool_hashes = {str(p.resolve()): sha(p) for p in tool_paths}
    (evidence / "tool-manifest-before.json").write_text(json.dumps(tool_hashes, indent=2) + "\n")
    installed_sources = sorted((Path(sysroot) / "lib/rustlib/src/rust").rglob("*.rs"))
    require(installed_sources, "rust-src is required to bind macro source aliases")
    installed_hashes = {str(p.resolve()): sha(p) for p in installed_sources}
    (evidence / "installed-source-manifest.json").write_text(json.dumps(installed_hashes, indent=2) + "\n")
    dependency_metadata = json.loads(run("dependency-metadata", ["cargo", "metadata",
        "--locked", "--manifest-path", helper / "Cargo.toml", "--format-version", "1"]).stdout)
    require(all(p["id"] == dependency_metadata["resolve"]["root"] or p.get("source")
                for p in dependency_metadata["packages"]), "unverified path dependency")
    packages = {p["id"]: p for p in dependency_metadata["packages"] if p.get("source")}
    dependency_hashes, dependency_roots, package_inputs = {}, {}, {}
    lock = (helper / "Cargo.lock").read_text()
    for identity, package in packages.items():
        require(package["source"].startswith("registry+"), "unverified dependency source kind")
        package_root = Path(package["manifest_path"]).parent.resolve()
        blocks = [b for b in lock.split("[[package]]")
                  if f'name = "{package["name"]}"' in b and f'version = "{package["version"]}"' in b]
        require(len(blocks) == 1, "ambiguous locked dependency package")
        checksum = re.search(r'^checksum = "([0-9a-f]{64})"$', blocks[0], re.MULTILINE)
        require(checksum is not None, "dependency lacks a locked archive checksum")
        archive = package_root.parent.parent.parent / "cache" / package_root.parent.name / (
            package["name"] + "-" + package["version"] + ".crate")
        require(sha(archive) == checksum[1], "dependency archive does not match locked checksum")
        package_inputs.update({str(p): sha(p) for p in (archive, Path(package["manifest_path"]))})
        dependency_roots[identity] = package_root
        archived_sources = set()
        with tarfile.open(archive, "r:gz") as content:
            for member in content:
                if not member.isfile():
                    continue
                relative = Path(member.name)
                require(not relative.is_absolute() and ".." not in relative.parts
                        and relative.parts[0] == package_root.name, "unexpected dependency archive path")
                relative = Path(*relative.parts[1:])
                if relative.suffix != ".rs" and str(relative) != "Cargo.toml":
                    continue
                path = (package_root / relative).resolve()
                require(path.is_relative_to(package_root), "dependency source escapes package")
                stream = content.extractfile(member)
                require(stream is not None and sha(path) == hashlib.sha256(stream.read()).hexdigest(),
                        "dependency build input differs from its verified archive")
                if relative.suffix == ".rs":
                    archived_sources.add(str(path))
                    dependency_hashes[str(path)] = sha(path)
        same_members((str(p.resolve()) for p in package_root.rglob("*.rs")), archived_sources,
                     "verified dependency Rust source population")
    (evidence / "dependency-source-manifest.json").write_text(json.dumps({
        "packages": packages, "source_sha256": dependency_hashes, "package_inputs": package_inputs}, indent=2) + "\n")
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
    flags = ["-C", "instrument-coverage", "-C", "llvm-args=-instrprof-atomic-counter-update-all",
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
    built = run("build-units", [cargo[0], "test", *cargo[1:], "--all-targets",
                                "--no-run", "--message-format=json"], env)
    messages = [json.loads(line) for line in built.stdout.splitlines() if line.startswith("{")]
    tests = [(message["target"]["name"], Path(message["executable"]))
             for message in messages
             if message.get("executable") and message.get("profile", {}).get("test")
             and message.get("target", {}).get("test")]
    require({name for name, _ in tests} == {"pty-runtime-guardian", "startup", "fault_recovery"},
            "missing or unexpected helper test executable")
    binary = work / "target/debug/pty-runtime-guardian"
    objects = [binary, *(path for _, path in tests)]
    compiled = set().union(*(dep_sources(path, helper) for path in objects))
    dependency_compiled, dependency_depfiles = set(), {}
    for identity, package_root in dependency_roots.items():
        artifacts = [m for m in messages if m.get("reason") == "compiler-artifact"
                     and m["package_id"] == identity and "lib" in m["target"]["kind"]]
        require(len(artifacts) == 1, "ambiguous selected dependency library artifact")
        libraries = [Path(p) for p in artifacts[0]["filenames"] if p.endswith(".rlib")]
        require(len(libraries) == 1, "missing selected dependency library")
        library = libraries[0]
        depfile = library.with_name(library.name.removeprefix("lib")).with_suffix(".d")
        selected = dep_sources(depfile, helper)
        require(all(p.is_relative_to(package_root) and str(p) in dependency_hashes for p in selected),
                "dependency artifact references unbound source")
        dependency_compiled.update(map(str, selected))
        dependency_depfiles[str(depfile)] = sha(depfile)
    (evidence / "dependency-compiled-sources.json").write_text(json.dumps({
        "source_files": sorted(dependency_compiled), "depfile_sha256": dependency_depfiles}, indent=2) + "\n")
    object_hashes = {str(p): sha(p) for p in [*objects, work / "target/debug/guardian-fixture", filename_object]}
    (evidence / "binary-manifest-before.json").write_text(json.dumps(object_hashes, indent=2) + "\n")
    # Build-script execution is not part of the runtime test population.
    for profile in profiles.glob("*.profraw"):
        profile.unlink()
    results = [run("test-" + name, [path, "--test-threads=1"], env, required=False)
               for name, path in tests]
    results.append(run("actual-protocol", [sys.executable, snapshot / "scripts/guardian/probe.py",
                                          "--helper", binary], env, required=False))
    raw = sorted(profiles.glob("*.profraw"))
    if not raw:
        raise SystemExit("No runtime profiles; no coverage claim permitted")
    merged = work / "guardian.profdata"
    profile_hashes = {str(p): sha(p) for p in raw}
    raw_records = {str(p): parse_profile(run(f"profile-raw-{i:03d}",
                    [llvm / "llvm-profdata", "show", "--all-functions", "--counts", "--text", p]).stdout)
                   for i, p in enumerate(raw)}
    run("merge", [llvm / "llvm-profdata", "merge", "-sparse", *raw, "-o", merged])
    conservation = profile_conservation(raw_records, parse_profile(run("profile-merged",
        [llvm / "llvm-profdata", "show", "--all-functions", "--counts", "--text", merged]).stdout))
    require(not conservation["saturated"], "saturated physical coverage counters")
    (evidence / "physical-counter-checks.json").write_text(json.dumps(conservation, indent=2) + "\n")
    files = sorted((helper / "src").glob("*.rs"))
    files.append(helper / "src/../../../scripts/guardian/protocol.rs")
    # Export both entire objects, then audit their complete source inventory.
    # LLVM normalizes source filters inconsistently for Rust #[path] modules.
    common = [binary]
    for _, path in tests:
        common += ["-object", path]
    common += [f"-instr-profile={merged}"]
    run("report", [llvm / "llvm-cov", "report", *common])
    exported = run("export", [llvm / "llvm-cov", "export", *common])
    data = json.loads(exported.stdout)
    audited, aliases = inventory(data, compiled, set(installed_hashes) | dependency_compiled)
    measured = {Path(f["filename"]).resolve() for f in audited["files"]}
    expected = {p.resolve() for p in files}
    absent = expected - measured
    (evidence / "denominator.json").write_text(json.dumps({
        "measured": sorted(map(str, measured)), "unmeasured_platform_source": sorted(map(str, absent))
    }, indent=2) + "\n")
    if absent != {helper / "src/discovery_linux.rs"}:
        raise SystemExit("Unexpected helper source denominator; inspect inventory")
    (evidence / "source-aliases.json").write_text(json.dumps(aliases, indent=2) + "\n")
    run("annotated", [llvm / "llvm-cov", "show", "-show-branches=count", *common])
    sanity = export_saturation(audited)
    saturated = sanity["saturated"]
    (evidence / "counter-sanity.json").write_text(json.dumps({
        "saturated_regions": saturated, "saturation_detected": bool(saturated),
        "scope": "Necessary counter sanity check, not sufficient proof of accurate coverage",
        "all_export_counts": sanity,
    }, indent=2) + "\n")
    (evidence / "binary-profile-manifest.json").write_text(json.dumps({str(p): sha(p)
        for p in [*objects, work / "target/debug/guardian-fixture", *raw]}, indent=2) + "\n")
    same_members(map(str, profiles.glob("*.profraw")), profile_hashes, "final profile population")
    require(all(sha(Path(p)) == value for p, value in
                {**object_hashes, **profile_hashes, **installed_hashes, **tool_hashes}.items()),
            "measured binary, profile, installed source or measurement tool changed")
    same_members((str(p.relative_to(snapshot)) for p in snapshot.rglob("*") if p.is_file()),
                 manifest, "final copied source population")
    same_members((str(p.resolve()) for p in (Path(sysroot) / "lib/rustlib/src/rust").rglob("*.rs")),
                 installed_hashes, "final installed source population")
    require(all(sha(snapshot / p) == value for p, value in manifest.items()), "copied source changed")
    require(all(sha(Path(p)) == value for p, value in {**dependency_hashes, **package_inputs,
                **dependency_depfiles}.items()), "dependency inputs changed")
    same_members((str(p.resolve()) for root in dependency_roots.values() for p in root.rglob("*.rs")),
                 dependency_hashes, "final selected dependency source population")
    if saturated:
        raise SystemExit("Saturated coverage counters; report is not trustworthy")
    if any(result.returncode for result in results):
        raise SystemExit("Coverage is partial evidence because a fixture failed")
    totals = audited["totals"]
    complete = all(totals[key]["count"] > 0 and totals[key]["covered"] == totals[key]["count"]
                   for key in ("lines", "functions", "regions"))
    (evidence / "readiness.json").write_text(json.dumps({"complete": complete,
        "totals": totals, "production_source_modified": False,
        "unmeasured_platform_source": sorted(map(str, absent)),
        "scope": "All selected helper/unit/startup/fault-recovery source and macro aliases; test code retained"}, indent=2) + "\n")
    if args.require_complete and not complete:
        raise SystemExit("Guardian 100% line/function/region target not met")


if __name__ == "__main__":
    main()
