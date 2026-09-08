#!/usr/bin/env python3
"""Reject mismatched native source archives and changed build inputs.

Uses the experiment's existing archive cache. Does not download or silently
substitute a library. CI bootstrap must build the pinned archive before Cargo.
"""
import hashlib
import pathlib
import sys
import tarfile

source = pathlib.Path(sys.argv[1]).resolve()
archive = source.parent / (source.name + ".tar.gz")
expected = "820d84e8cbc4be0ca26b9a1b71cfdc5befdc814c414bef7b0dbcf51069668c68"
if not archive.is_file() or hashlib.sha256(archive.read_bytes()).hexdigest() != expected:
    raise SystemExit("Ghostty source archive does not match reviewed SHA-256")
with tarfile.open(archive, "r:gz") as content:
    for member in content:
        if not member.isfile():
            continue
        relative = pathlib.PurePosixPath(member.name)
        if relative.parts[0] != "ghostty-82232ecde55405559dec29c5466cb9e39938cb41":
            raise SystemExit("Unexpected Ghostty archive root")
        target = source.joinpath(*relative.parts[1:])
        if target.suffix not in (".zig", ".zon", ".h", ".c", ".cpp", ".S"):
            continue
        stream = content.extractfile(member)
        if stream is None or not target.is_file() or target.read_bytes() != stream.read():
            raise SystemExit("Ghostty build input differs from pinned archive")
