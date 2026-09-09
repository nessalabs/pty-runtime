#!/usr/bin/env python3
"""Verify the pinned archive and apply only the reviewed snapshot roundtrip patch."""
import argparse
import hashlib
import json
import pathlib
import subprocess
import tarfile
import tempfile

ARCHIVE_SHA256 = "820d84e8cbc4be0ca26b9a1b71cfdc5befdc814c414bef7b0dbcf51069668c68"
ARCHIVE_ROOT = "ghostty-82232ecde55405559dec29c5466cb9e39938cb41"
# Build inputs changed by this fixed, reviewed patch.
TARGET_HASHES = {
    pathlib.Path("src/terminal/snapshot/screen.zig"): (
        "abc550e1b8cbee843f2ee5b2168602aff1ef66b11368f5c38394b7b27704fca7",
        "4ae17bd3be6851083d4e60f8378ece70f6910a6e9de75a2fdfa1c9afb3beb819"),
    pathlib.Path("src/terminal/PageList.zig"): (
        "cd926e56749c014a8df7f30fff1f5c32548cb4a8731451e726d7171d25818fcb",
        "a7703d31bfc95c68446e466ba3cf5cc329bed405e3527fe5503d344547e50462"),
    pathlib.Path("src/terminal/bitmap_allocator.zig"): (
        "bac61a65b5a3141ccfad2d9d0a6a452be7106a647182470fcf38e1289b5f86e1",
        "32673b2a73f1cf5135fbb1aa4f07855bff6a0c3e42789a178e9ee93da046debc"),
    pathlib.Path("src/terminal/snapshot/page.zig"): (
        "2e58c7f15983cd365fc3b7f1aa7e28b515f3fe40654e950c1175761ed2acca6e",
        "3d10248e58c4b4019ff463e6bcc829fe808e82b9c88a448433d4cc7a178f5052"),
}
PREVIOUS_PAGE_LIST_SHA256S = (
    "8b844ab0976ecf9551db24508d7e934d9aa9739d79849baeb0c8492de63ce393",
    "a288b692c579a47411affc8d543e89d57690a587bc4e7eebe6d95c9010fd3aa7",
)
PATCH_SHA256 = "2f94f382946d59e97b6d03b625092964e8cb62662acff75e9aec09866d4b71e7"
PATCH = pathlib.Path(__file__).resolve().parent / "patches/snapshot-pending-wrap.patch"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def verify(source, prepare=False, built=False):
    source = pathlib.Path(source).resolve()
    archive = source.parent / (source.name + ".tar.gz")
    require(archive.is_file() and sha(archive.read_bytes()) == ARCHIVE_SHA256,
            "Ghostty source archive does not match reviewed SHA-256")
    require(sha(PATCH.read_bytes()) == PATCH_SHA256, "Ghostty patch does not match reviewed SHA-256")
    originals = {}
    needs_patch = False
    with tarfile.open(archive, "r:gz") as content:
        for member in content:
            if not member.isfile():
                continue
            relative = pathlib.PurePosixPath(member.name)
            require(relative.parts[0] == ARCHIVE_ROOT and ".." not in relative.parts,
                    "Unexpected Ghostty archive path")
            relative = pathlib.Path(*relative.parts[1:])
            target = source / relative
            if target.suffix not in (".zig", ".zon", ".h", ".c", ".cpp", ".S"):
                continue
            stream = content.extractfile(member)
            require(stream is not None and target.is_file(), "Missing Ghostty build input")
            original = stream.read()
            actual = target.read_bytes()
            if relative in TARGET_HASHES:
                before, after = TARGET_HASHES[relative]
                require(sha(original) == before, "Unexpected patch source in pinned archive")
                allowed = (before, after) if prepare else (after,)
                if prepare and relative == pathlib.Path("src/terminal/PageList.zig"):
                    allowed += PREVIOUS_PAGE_LIST_SHA256S
                require(sha(actual) in allowed, "Ghostty snapshot input is not a reviewed patch state")
                originals[relative] = original
                needs_patch |= sha(actual) != after
            else:
                require(actual == original, "Ghostty build input differs from pinned archive")
    require(originals.keys() == TARGET_HASHES.keys(), "Pinned archive lacks reviewed patch inputs")
    if prepare and needs_patch:
        # Stage original files so any previously applied narrower patch
        # state and a fresh archive use the same operation.
        with tempfile.TemporaryDirectory() as directory:
            for relative, original in originals.items():
                staged = pathlib.Path(directory) / relative
                staged.parent.mkdir(parents=True, exist_ok=True)
                staged.write_bytes(original)
            subprocess.run(["patch", "-s", "-p1", "-i", str(PATCH)], cwd=directory, check=True)
            results = {relative: (pathlib.Path(directory) / relative).read_bytes()
                       for relative in TARGET_HASHES}
            for relative, result in results.items():
                require(sha(result) == TARGET_HASHES[relative][1], "Ghostty patch produced unexpected source")
            for relative, result in results.items():
                target = source / relative
                if target.read_bytes() != result:
                    target.write_bytes(result)
    if built:
        stamp = json.loads((source / "experiment-build.json").read_text())
        require(stamp.get("snapshot_patch_sha256") == PATCH_SHA256,
                "Ghostty library build does not include reviewed snapshot patch")
        library = source / "zig-out/lib/libghostty-vt.a"
        require(stamp.get("library_sha256") == sha(library.read_bytes()),
                "Ghostty library differs from recorded patched build")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=pathlib.Path)
    parser.add_argument("--prepare", action="store_true")
    parser.add_argument("--built", action="store_true")
    args = parser.parse_args()
    try:
        verify(args.source, args.prepare, args.built)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        raise SystemExit(str(error)) from error


if __name__ == "__main__":
    main()
