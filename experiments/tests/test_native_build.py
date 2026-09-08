"""Native command/cache contract; no download or expensive native compile needed."""
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import run


class NativeBuildContract(unittest.TestCase):
    def exercise_cache(self, key):
        with tempfile.TemporaryDirectory() as temporary:
            cache = Path(temporary)
            source = cache / ('ghostty-' + run.DEPENDENCIES['ghostty']['revision'])
            source.mkdir()
            (source / 'build.zig').touch()
            zig = cache / ('zig-' + key + '-' + run.DEPENDENCIES['zig']['version'])
            zig.mkdir()
            (zig / 'zig').touch()
            commands = []

            def compile_fixture(argv, **kwargs):
                commands.append(list(map(str, argv)))
                if str(argv[0]).endswith('/zig') and argv[1] == 'build':
                    library = source / 'zig-out/lib/libghostty-vt.a'
                    library.parent.mkdir(parents=True, exist_ok=True)
                    library.touch()

            with patch.object(run, 'fetch'), patch.object(run, 'capture', return_value='fixture'), \
                    patch.object(run, 'checked', side_effect=compile_fixture), \
                    patch.object(run, 'target_key', return_value=key), \
                    patch.object(run.shutil, 'which', return_value='/fixture/cc'):
                run.build(cache, 'native', 2)
                native = [c for c in commands if c[1] == 'build']
                self.assertEqual(len(native), 1)
                self.assertIn('-Dcpu=baseline', native[0])
                self.assertNotIn('-Dcpu=native', native[0])
                stamp = source / 'experiment-build.json'
                saved = json.loads(stamp.read_text())
                self.assertIn('-Dcpu=baseline', saved['build_options'])
                self.assertEqual(saved['build_driver_sha256'], run.sha(Path(run.__file__)))
                self.assertEqual(saved['snapshot_patch_sha256'], run.sha(run.ROOT / 'scripts/native/patches/snapshot-pending-wrap.patch'))
                self.assertTrue(any(c[-1] == '--prepare' for c in commands))
                run.build(cache, 'native', 2)
                self.assertEqual(sum(c[1] == 'build' for c in commands), 1)
                # Both CPU target and recipe changes invalidate a compiled archive.
                for field, value in [('build_options', ['-Dcpu=native']), ('build_driver_sha256', 'old-driver'), ('snapshot_patch_sha256', 'old-patch'), ('source_verifier_sha256', 'old-verifier'), ('library_sha256', 'old-library')]:
                    changed = dict(saved)
                    changed[field] = value
                    stamp.write_text(json.dumps(changed))
                    before = sum(c[1] == 'build' for c in commands)
                    run.build(cache, 'native', 2)
                    self.assertEqual(sum(c[1] == 'build' for c in commands), before + 1)
                # Legacy pre-CPU-policy metadata cannot skip the rebuild.
                stamp.write_text(json.dumps({'target': key, 'optimize': 'ReleaseFast'}))
                before = sum(c[1] == 'build' for c in commands)
                run.build(cache, 'native', 2)
                self.assertEqual(sum(c[1] == 'build' for c in commands), before + 1)

    def test_linux_build_and_cache_include_baseline_cpu_and_driver(self):
        self.exercise_cache('x86_64-linux')

    def test_macos_build_and_cache_include_baseline_cpu_and_driver(self):
        self.exercise_cache('aarch64-macos')
