"""The native patch accepts only reviewed source states and a recorded library."""
import contextlib
import difflib
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    "native_source", Path(__file__).resolve().parents[1] / "native/verify_source.py")
native = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(native)


class SourceVerification(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        root = Path(self.directory.name)
        self.source = root / native.ARCHIVE_ROOT
        self.source.mkdir()
        self.original = b"cursor pending wrap normalized\n"
        self.corrected = b"cursor pending wrap preserved\n"
        self.targets = tuple(native.TARGET_HASHES)
        archive = root / (native.ARCHIVE_ROOT + ".tar.gz")
        with tarfile.open(archive, "w:gz") as output:
            for relative, data in [(str(target), self.original) for target in self.targets] + [("build.zig", b"build\n")]:
                member = tarfile.TarInfo(native.ARCHIVE_ROOT + "/" + relative)
                member.size = len(data)
                output.addfile(member, io.BytesIO(data))
                target = self.source / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(data)
        patch_file = root / "snapshot.patch"
        patch_file.write_text("".join(line for target in self.targets for line in difflib.unified_diff(
            self.original.decode().splitlines(True), self.corrected.decode().splitlines(True),
            fromfile="a/" + str(target), tofile="b/" + str(target))))
        stack = contextlib.ExitStack()
        self.addCleanup(stack.close)
        for name, value in {"ARCHIVE_SHA256": native.sha(archive.read_bytes()),
                            "TARGET_HASHES": {target: (native.sha(self.original), native.sha(self.corrected))
                                              for target in self.targets},
                            "PATCH_SHA256": native.sha(patch_file.read_bytes()),
                            "PATCH": patch_file}.items():
            stack.enter_context(patch.object(native, name, value))

    def test_prepare_is_exact_and_idempotent(self):
        with self.assertRaisesRegex(ValueError, "reviewed patch state"):
            native.verify(self.source)
        native.verify(self.source, prepare=True)
        self.assertEqual((self.source / self.targets[0]).read_bytes(), self.corrected)
        native.verify(self.source, prepare=True)
        native.verify(self.source)

    def test_cursor_only_patch_upgrades_to_complete_patch(self):
        (self.source / self.targets[0]).write_bytes(self.corrected)
        native.verify(self.source, prepare=True)
        for target in self.targets:
            self.assertEqual((self.source / target).read_bytes(), self.corrected)
        native.verify(self.source)

    def test_growth_only_patch_upgrades_to_wide_cutoff_patch(self):
        previous = b"previous reviewed growth correction\n"
        (self.source / self.targets[0]).write_bytes(self.corrected)
        (self.source / self.targets[1]).write_bytes(previous)
        with patch.object(native, "PREVIOUS_PAGE_LIST_SHA256S", (native.sha(previous),)):
            with self.assertRaisesRegex(ValueError, "reviewed patch state"):
                native.verify(self.source)
            native.verify(self.source, prepare=True)
        native.verify(self.source)

    def test_unexpected_second_result_does_not_publish_first_file(self):
        hashes = dict(native.TARGET_HASHES)
        hashes[self.targets[1]] = (native.sha(self.original), "unexpected")
        with patch.object(native, "TARGET_HASHES", hashes):
            with self.assertRaisesRegex(ValueError, "unexpected source"):
                native.verify(self.source, prepare=True)
        for target in self.targets:
            self.assertEqual((self.source / target).read_bytes(), self.original)

    def test_archive_patch_and_unrelated_source_tampering_fail(self):
        for target, message in [(native.PATCH, "patch does not match"),
                                (self.source / "build.zig", "differs from pinned"),
                                (self.source / self.targets[0], "reviewed patch state"),
                                (self.source.with_suffix(".tar.gz"), "archive does not match")]:
            with self.subTest(target=target):
                original = target.read_bytes()
                target.write_bytes(original + b"tampered")
                try:
                    with self.assertRaisesRegex(ValueError, message):
                        native.verify(self.source, prepare=True)
                    self.assertEqual((self.source / self.targets[0]).read_bytes(),
                                     self.original + (b"tampered" if target == self.source / self.targets[0] else b""))
                finally:
                    target.write_bytes(original)

    def test_library_must_match_patched_build_record(self):
        native.verify(self.source, prepare=True)
        library = self.source / "zig-out/lib/libghostty-vt.a"
        library.parent.mkdir(parents=True)
        library.write_bytes(b"library")
        stamp = self.source / "experiment-build.json"
        stamp.write_text(json.dumps({"snapshot_patch_sha256": "old"}))
        with self.assertRaisesRegex(ValueError, "does not include"):
            native.verify(self.source, built=True)
        stamp.write_text(json.dumps({"snapshot_patch_sha256": native.PATCH_SHA256,
                                     "library_sha256": native.sha(library.read_bytes())}))
        native.verify(self.source, built=True)
        library.write_bytes(b"stale or replaced library")
        with self.assertRaisesRegex(ValueError, "differs from recorded"):
            native.verify(self.source, built=True)
