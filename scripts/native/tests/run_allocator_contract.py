#!/usr/bin/env python3
"""Compile and run the real bridge allocator callback contract after bootstrap."""
import argparse
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[3]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--sanitize", action="store_true")
args = parser.parse_args()
source = Path(os.environ.get("PTY_RUNTIME_GHOSTTY_SOURCE", str(
    ROOT / "work/experiment-cache/ghostty-82232ecde55405559dec29c5466cb9e39938cb41")))
subprocess.run([sys.executable, str(ROOT / "scripts/native/verify_source.py"),
                str(source), "--built"], check=True)
compiler = shutil.which("clang") or shutil.which("cc")
if compiler is None:
    raise SystemExit("C compiler required")
with tempfile.TemporaryDirectory(prefix="pty-allocator-contract-") as directory:
    binary = Path(directory) / "allocator-contract"
    command = [compiler, "-std=c11", "-O1", "-g", "-Wall", "-Wextra", "-Werror"]
    if args.sanitize:
        command += ["-fno-omit-frame-pointer", "-fsanitize=address,undefined",
                    "-fno-sanitize-recover=all"]
    command += ["-I", str(source / "include"),
                str(Path(__file__).with_name("allocator-contract.c")),
                str(ROOT / "scripts/native/owner.c"),
                str(source / "zig-out/lib/libghostty-vt.a")]
    if platform.system() == "Linux":
        command += ["-lm", "-lpthread", "-ldl"]
    command += ["-o", str(binary)]
    print(repr(command), flush=True)
    subprocess.run(command, check=True)
    subprocess.run([str(binary)], check=True)
