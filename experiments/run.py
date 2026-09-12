#!/usr/bin/env python3
"""Run portable PTY/native experiments and gate complete, comparable results."""
from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import os
import platform
import resource
import shutil
import signal
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.request
from pathlib import Path

import gate

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
DEPENDENCIES = json.loads((HERE / "dependencies.json").read_text())
PROFILES = json.loads((HERE / "profiles.json").read_text())


def save(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, indent=2, sort_keys=True, allow_nan=False) + "\n")
    temporary.replace(path)


def sha(path):
    h = hashlib.sha256()
    with Path(path).open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def capture(argv):
    return subprocess.check_output(argv, text=True, stderr=subprocess.STDOUT, timeout=30).strip()


def checked(argv, *, cwd=None, env=None, timeout=600, log=None):
    """A timeout kills the whole fixture process group, including its producers."""
    proc = subprocess.Popen([str(a) for a in argv], cwd=cwd, env=env, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
    try:
        stdout, stderr = proc.communicate(timeout=timeout)
    except (subprocess.TimeoutExpired, KeyboardInterrupt):
        os.killpg(proc.pid, signal.SIGKILL)
        stdout, stderr = proc.communicate()
        if log:
            Path(str(log) + ".stdout").write_text(stdout)
            Path(str(log) + ".stderr").write_text(stderr + "\nPROCESS GROUP TERMINATED\n")
        raise
    if log:
        Path(str(log) + ".stdout").write_text(stdout)
        Path(str(log) + ".stderr").write_text(stderr)
    if proc.returncode:
        raise RuntimeError(f"command exited {proc.returncode}: {' '.join(map(str, argv))}\n{stderr[-6000:]}")
    return stdout


def fetch(url, checksum, destination):
    destination = Path(destination)
    if destination.exists():
        gate.require(sha(destination) == checksum, f"cached archive hash mismatch: {destination}")
        return
    print(f"Downloading pinned dependency: {url}", flush=True)
    temporary = destination.with_suffix(destination.suffix + ".part")
    with urllib.request.urlopen(url, timeout=60) as source, temporary.open("wb") as target:
        shutil.copyfileobj(source, target)
    gate.require(sha(temporary) == checksum, f"download hash mismatch: {url}")
    temporary.replace(destination)


def extract(archive, directory):
    # Keep macOS's older system Python usable without allowing archive path escapes.
    with tarfile.open(archive) as content:
        if hasattr(tarfile, "data_filter"):
            content.extractall(directory, filter="data")
            return
        root = Path(directory).resolve()
        for member in content.getmembers():
            target = root / member.name
            gate.require(not Path(member.name).is_absolute() and target.resolve().is_relative_to(root),
                         "archive path escapes cache")
            gate.require(member.isfile() or member.isdir() or member.issym() or member.islnk(),
                         "unsupported archive member")
            if member.issym() or member.islnk():
                link = (target.parent if member.issym() else root) / member.linkname
                gate.require(link.resolve().is_relative_to(root), "archive link escapes cache")
            member.mode &= 0o777
            content.extract(member, root)


def target_key():
    machine = {"arm64": "aarch64", "aarch64": "aarch64", "x86_64": "x86_64"}.get(platform.machine())
    system = {"Darwin": "macos", "Linux": "linux"}.get(platform.system())
    gate.require(machine and system, "experiments support macOS/Linux on arm64/x86_64")
    return machine + "-" + system


# Cached native archives must run across different CPUs of the same architecture.
# Zig's omitted CPU target detects the build host and is unsafe for shared caches.
NATIVE_BUILD_OPTIONS = ("-Demit-lib-vt", "-Demit-xcframework=false", "-Doptimize=ReleaseFast", "-Dcpu=baseline")


def native_build_stamp(key):
    return {"dependencies": DEPENDENCIES, "target": key, "libc": platform.libc_ver(),
            "build_options": list(NATIVE_BUILD_OPTIONS), "build_driver_sha256": sha(Path(__file__)),
            "snapshot_patch_sha256": sha(ROOT / "scripts/native/patches/snapshot-pending-wrap.patch"),
            "source_verifier_sha256": sha(ROOT / "scripts/native/verify_source.py")}


def build(cache, suite, jobs):
    cache.mkdir(parents=True, exist_ok=True)
    logs = cache / "build-logs"
    logs.mkdir(exist_ok=True)
    binaries = {}
    versions = {}
    if suite in ("all", "pty"):
        gate.require(shutil.which("cargo") is not None, "cargo is required for PTY experiments")
        print("Building the locked Rust PTY fixture...", flush=True)
        env = dict(os.environ, CARGO_TARGET_DIR=str(cache / "rust-target"))
        checked(["cargo", "build", "--release", "--locked", "--jobs", str(jobs)],
                cwd=HERE / "pty", env=env, log=logs / "rust")
        binaries["pty"] = cache / "rust-target/release/pty-experiment-harness"
        versions["rustc"] = capture(["rustc", "-Vv"])
    if suite in ("all", "native"):
        key = target_key()
        version = DEPENDENCIES["zig"]["version"]
        zig_archive = cache / f"zig-{key}-{version}.tar.xz"
        fetch(f"https://ziglang.org/download/{version}/zig-{key}-{version}.tar.xz",
              DEPENDENCIES["zig"]["archives"][key], zig_archive)
        zig_root = cache / f"zig-{key}-{version}"
        if not (zig_root / "zig").is_file():
            extract(zig_archive, cache)
        zig = zig_root / "zig"
        source = cache / ("ghostty-" + DEPENDENCIES["ghostty"]["revision"])
        archive = cache / (source.name + ".tar.gz")
        fetch(DEPENDENCIES["ghostty"]["url"], DEPENDENCIES["ghostty"]["sha256"], archive)
        if not (source / "build.zig").exists():
            extract(archive, cache)
        checked([sys.executable, ROOT / "scripts/native/verify_source.py", source, "--prepare"])
        stamp_value = native_build_stamp(key)
        stamp = source / "experiment-build.json"
        library = source / "zig-out/lib/libghostty-vt.a"
        if library.is_file():
            stamp_value["library_sha256"] = sha(library)
        if not library.is_file() or not stamp.exists() or json.loads(stamp.read_text()) != json.loads(json.dumps(stamp_value)):
            print(f"Building pinned libghostty-vt for {key}...", flush=True)
            checked([zig, "build", *NATIVE_BUILD_OPTIONS, f"-j{jobs}",
                     "--global-cache-dir", cache / "zig-global-cache"], cwd=source,
                    timeout=1800, log=logs / "ghostty")
            gate.require(library.is_file(), "native build did not emit the static VT library")
            stamp_value["library_sha256"] = sha(library)
            save(stamp, stamp_value)
        compiler = shutil.which("clang") or shutil.which("cc")
        gate.require(compiler is not None, "a C compiler is required for native experiments")
        destination = cache / "native-bin"
        destination.mkdir(exist_ok=True)
        for name in ("native-memory", "codec-timing", "verify-roundtrip"):
            command = [compiler, "-O3", "-Wall", "-Wextra", "-I", str(source / "include"),
                       str(HERE / "native" / (name + ".c")), str(library)]
            if platform.system() == "Linux":
                command += ["-lm", "-lpthread", "-ldl"]
            command += ["-o", str(destination / name)]
            checked(command, log=logs / name)
            binaries[name] = destination / name
        versions["zig"] = capture([zig, "version"])
        versions["cc"] = capture([compiler, "--version"])
    return binaries, versions


def cases_for(config, suite):
    cases = []

    def add(identity, kind, binary, args):
        cases.append({"id": identity, "kind": kind, "binary": binary, "args": list(map(str, args))})

    if suite in ("all", "pty"):
        b = config["read_bytes"]
        for n in config["idle_counts"]:
            for model in config["models"]:
                add(f"pty-idle-n{n}-{model}", "pty-idle", "pty", ["idle", model, n, b, config["idle_ms"]])
        for case in config["serial"]:
            n = case["ptys"]
            for model in config["models"]:
                add(f"pty-serial-n{n}-{model}", "pty-serial", "pty",
                    ["serial", model, n, b, case["mib_per_pty"]])
        for case in config["concurrent"]:
            n, active, rate = case["ptys"], case["active"], case["rate_per_producer"]
            for model in config["models"]:
                add(f"pty-concurrent-n{n}-a{active}-rate{rate}-{model}", "pty-concurrent", "pty",
                    ["concurrent", model, n, b, active, case["duration_ms"], rate])
        # Each handoff case names its own shared placement target, because the
        # measurement compares dedicated readers against that target in one
        # process rather than crossing a model list.
        for case in config.get("handoff", []):
            n, active, rate = case["ptys"], case["active"], case["rate_per_producer"]
            model = case["model"]
            add(f"pty-handoff-n{n}-a{active}-rate{rate}-{model}", "pty-handoff", "pty",
                ["handoff", model, n, b, active, case["duration_ms"], rate,
                 case["cycles"], case["window_ms"], case["probe_ms"]])
    if suite in ("all", "native"):
        for n in config["native_counts"]:
            for varied in config["native_varied"]:
                for compression, allocator in config["native_strategies"]:
                    add(f"native-n{n}-v{varied}-c{compression}-a{allocator}", "native-lifecycle", "native-memory",
                        [n, config["native_lines"], varied, compression, 8, 0, allocator])
        for lines in config["codec_lines"]:
            for varied in config["native_varied"]:
                add(f"codec-lines{lines}-v{varied}", "native-codec", "codec-timing", [lines, varied])
        add("native-correctness", "native-correctness", "verify-roundtrip", [])
    return cases


def read_optional(path):
    try:
        return Path(path).read_text().strip()
    except OSError:
        return None


def environment(versions):
    if platform.system() == "Darwin":
        model = capture(["sysctl", "-n", "hw.model"])
        cpu = capture(["sysctl", "-n", "machdep.cpu.brand_string"])
        ram = int(capture(["sysctl", "-n", "hw.memsize"]))
        container = {}
    else:
        model = "linux"
        cpu = next((line.split(":", 1)[1].strip() for line in Path("/proc/cpuinfo").read_text().splitlines()
                    if line.startswith("model name")), platform.processor())
        ram = os.sysconf("SC_PHYS_PAGES") * os.sysconf("SC_PAGE_SIZE")
        container = {name: read_optional("/sys/fs/cgroup/" + name)
                     for name in ("cpu.max", "memory.max", "pids.max", "cpuset.cpus.effective")}
    return {"system": platform.system(), "kernel": platform.release(), "architecture": platform.machine(),
            "machine_model": model, "cpu_model": cpu, "logical_cpus": os.cpu_count(), "ram_bytes": ram,
            "page_size": os.sysconf("SC_PAGE_SIZE"), "libc": list(platform.libc_ver()),
            "container_limits": container, "toolchains": versions}


def source_manifest():
    files = [p for p in HERE.rglob("*") if p.is_file() and
             (p.suffix in (".py", ".rs", ".c", ".h") or p.name in ("Cargo.toml", "Cargo.lock", "dependencies.json", "profiles.json"))
             and "target" not in p.parts and "__pycache__" not in p.parts]
    return {str(p.relative_to(ROOT)): sha(p) for p in sorted(files)}


def execute(args):
    output = args.output.resolve()
    gate.require(not output.exists(), f"result directory already exists: {output}")
    output.mkdir(parents=True)
    logs = output / "logs"
    logs.mkdir()
    config = PROFILES[args.profile]
    repetitions = args.repetitions or config["repetitions"]
    gate.require(repetitions > 0, "repetitions must be positive")
    cases = cases_for(config, args.suite)
    manifest = source_manifest()
    metadata = {"schema": gate.SCHEMA, "profile": args.profile, "suite": args.suite, "cases": cases,
                "repetitions": repetitions, "minimum_baseline_repetitions": config["minimum_baseline_repetitions"],
                "source_files": manifest, "source_sha256": gate.digest(manifest), "dependencies": DEPENDENCIES,
                "started_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
                "load_average_at_start": os.getloadavg(), "descriptor_limit": resource.getrlimit(resource.RLIMIT_NOFILE)}
    save(output / "metadata.json", metadata)
    records = []
    try:
        binaries, versions = build(args.cache.resolve(), args.suite, args.jobs)
        metadata["comparison_environment"] = environment(versions)
        metadata["binary_sha256"] = {key: sha(path) for key, path in binaries.items()}
        save(output / "metadata.json", metadata)
        total = len(cases) * repetitions
        with (output / "results.jsonl").open("w") as sink:
            for iteration in range(repetitions):
                ordered = cases if iteration % 2 == 0 else list(reversed(cases))
                for case in ordered:
                    index = len(records) + 1
                    print(f"[{index}/{total}] {case['id']} repetition {iteration+1}", flush=True)
                    command = [str(binaries[case["binary"]]), *case["args"]]
                    record = {"case_id": case["id"], "iteration": iteration, "command": command, "status": "failed"}
                    start = time.monotonic()
                    try:
                        with tempfile.TemporaryDirectory(prefix="pty-case-", dir=output) as scratch:
                            stdout = checked(command, cwd=scratch, timeout=args.timeout,
                                             log=logs / f"{index:04d}-{case['id']}")
                        record["data"] = [json.loads(line) for line in stdout.splitlines() if line.strip()]
                        gate.validate_record(case, record["data"])
                        record["status"] = "passed"
                    except Exception as error:
                        record["error"] = str(error)
                        raise
                    finally:
                        record["wall_seconds"] = time.monotonic() - start
                        records.append(record)
                        sink.write(json.dumps(record, allow_nan=False) + "\n")
                        sink.flush()
        summary = gate.summarize(metadata, records)
        save(output / "summary.json", summary)
        if args.baseline:
            comparison = gate.compare(json.loads(args.baseline.read_text()), summary)
            save(output / "gate.json", comparison)
            if not comparison["passed"]:
                print(f"Performance gate failed: {len(comparison['regressions'])} regressions", file=sys.stderr)
                return 1
        print(f"Passed {len(records)} fixtures. Results: {output}", flush=True)
        return 0
    except (Exception, KeyboardInterrupt) as error:
        save(output / "failure.json", {"complete": False, "error": str(error), "records_written": len(records)})
        print(f"Experiment run failed: {error}", file=sys.stderr)
        return 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    common = argparse.ArgumentParser(add_help=False)
    common.add_argument("--cache", type=Path, default=ROOT / "work/experiment-cache")
    common.add_argument("--suite", choices=("all", "pty", "native"), default="all")
    common.add_argument("--jobs", type=int, default=min(os.cpu_count() or 1, 4))
    sub.add_parser("build", parents=[common], help="fetch verified dependencies and build fixtures")
    run = sub.add_parser("run", parents=[common], help="run fixtures and validate structured evidence")
    run.add_argument("--profile", choices=tuple(PROFILES), default="smoke")
    run.add_argument("--repetitions", type=int)
    run.add_argument("--timeout", type=float, default=120, help="deadline for each fixture in seconds")
    run.add_argument("--output", required=True, type=Path)
    run.add_argument("--baseline", type=Path, help="also run a strict performance comparison")
    baseline = sub.add_parser("baseline", help="create an explicitly selected baseline from validated results")
    baseline.add_argument("--results", type=Path, required=True)
    baseline.add_argument("--output", type=Path, required=True)
    compare = sub.add_parser("compare", help="compare complete results with a matching baseline")
    compare.add_argument("--results", type=Path, required=True)
    compare.add_argument("--baseline", type=Path, required=True)
    compare.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.command == "build":
            binaries, _ = build(args.cache.resolve(), args.suite, args.jobs)
            print(json.dumps({key: str(path) for key, path in binaries.items()}, indent=2))
            return 0
        if args.command == "run":
            return execute(args)
        result = gate.load_results(args.results)
        if args.command == "baseline":
            gate.validate_baseline(result)
            gate.require(not args.output.exists(), "baseline exists; choose a new path for review")
            save(args.output, result)
            print(f"Baseline saved: {args.output}")
            return 0
        comparison = gate.compare(json.loads(args.baseline.read_text()), result)
        save(args.output, comparison)
        print(json.dumps({"passed": comparison["passed"], "metrics_checked": comparison["metrics_checked"],
                          "regressions": len(comparison["regressions"])}))
        return 0 if comparison["passed"] else 1
    except (ValueError, KeyError, OSError, subprocess.SubprocessError) as error:
        print(f"Invalid experiment input or incompatible results: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
