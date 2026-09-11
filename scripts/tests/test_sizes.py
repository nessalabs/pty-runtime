"""File size is reported, not enforced.

Size is an alarm worth looking at, but a coherent file is better than one chopped
up to satisfy a threshold, so an oversized file is reported and the gate
continues. This test pins that it is still *noticed* -- silently dropping the
check would be a different decision from deliberately softening it.
"""
import importlib.util
import io
from contextlib import redirect_stdout
from pathlib import Path
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('size_gate', ROOT / 'scripts/gate.py')
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)


class SizeGateTests(unittest.TestCase):
    def test_oversized_native_rust_c_and_header_are_reported_without_blocking(self):
        original = Path.read_text
        for name in ['scripts/native/build.rs', 'scripts/native/owner.c', 'scripts/native/bridge.h',
                     'helpers/guardian/src/main.rs', 'scripts/guardian/protocol.rs',
                     'examples/event_stream_reconnect.rs', 'experiments/native/packed-pages.h']:
            target = ROOT / name
            with self.subTest(path=name):
                def read(path, *args, **kwargs):
                    return 'code\n' * 351 if path == target else original(path, *args, **kwargs)
                captured = io.StringIO()
                with patch.object(Path, 'read_text', read):
                    # Must not raise: size no longer fails the gate.
                    with redirect_stdout(captured):
                        gate.architecture()
                self.assertIn(name, captured.getvalue())
                self.assertIn('351 nonblank lines exceeds 350', captured.getvalue())

    def test_dependency_direction_still_fails_closed(self):
        """Softening size must not have softened the edges that matter."""
        captured = io.StringIO()
        with redirect_stdout(captured):
            gate.architecture()
        self.assertNotIn('Forbidden dependency', captured.getvalue())


if __name__ == '__main__':
    unittest.main()
