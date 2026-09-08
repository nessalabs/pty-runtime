#!/usr/bin/env python3
"""Run own C bridge error/cleanup contracts using explicit external-call faults."""
import argparse
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[3]
BRIDGE = ("owner", "checkpoint", "view", "verification")
REDIRECT = ("malloc", "calloc", "posix_memalign", "free", "ghostty_terminal_set",
            "ghostty_terminal_get", "ghostty_terminal_vt_write", "ghostty_terminal_compress", "ghostty_terminal_resize",
            "ghostty_grid_ref_style", "ghostty_grid_ref_cell", "ghostty_cell_get",
            "ghostty_grid_ref_graphemes", "ghostty_snapshot_encode",
            "ghostty_snapshot_decoder_set", "ghostty_snapshot_decoder_get",
            "ghostty_snapshot_decoder_next", "ghostty_formatter_terminal_new")


def build(compiler, source, root, directory, extra=()):
    common = [str(compiler), "-std=c11", "-O0", "-g", "-Wall", "-Wextra", "-Werror",
              "-I", str(source / "include"), *extra]
    objects = []
    for name in BRIDGE:
        obj = directory / (name + ".o")
        subprocess.run([*common, *[f"-D{n}=test_{n}" for n in REDIRECT], "-c",
                        str(root / "scripts/native" / (name + ".c")), "-o", str(obj)], check=True)
        objects.append(str(obj))
    binary = directory / "boundary-contract"
    command = [*common, *objects, str(root / "scripts/native/tests/boundary-shim.c"),
               str(root / "scripts/native/tests/boundary-contract.c"),
               str(source / "zig-out/lib/libghostty-vt.a")]
    if platform.system() == "Linux":
        command += ["-lm", "-lpthread", "-ldl"]
    subprocess.run([*command, "-o", str(binary)], check=True)
    return binary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sanitize", action="store_true")
    parser.add_argument("--build-only", type=Path)
    args = parser.parse_args()
    source = Path(os.environ.get("PTY_RUNTIME_GHOSTTY_SOURCE", str(
        ROOT / "work/experiment-cache/ghostty-82232ecde55405559dec29c5466cb9e39938cb41")))
    subprocess.run([sys.executable, str(ROOT / "scripts/native/verify_source.py"),
                    str(source), "--built"], check=True)
    compiler = os.environ.get("CC") or shutil.which("clang") or shutil.which("cc")
    if compiler is None:
        raise SystemExit("C compiler required")
    extra = ["-fno-omit-frame-pointer", "-fsanitize=address,undefined",
             "-fno-sanitize-recover=all"] if args.sanitize else []
    if args.build_only:
        args.build_only.mkdir(parents=True, exist_ok=True)
        build(compiler, source, ROOT, args.build_only, extra)
        return
    with tempfile.TemporaryDirectory(prefix="pty-boundary-contract-") as directory:
        binary = build(compiler, source, ROOT, Path(directory), extra)
        subprocess.run([str(binary)], check=True)


if __name__ == "__main__":
    main()
