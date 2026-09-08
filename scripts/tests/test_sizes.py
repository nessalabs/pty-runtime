"""Production native and Rust size enforcement must fail closed."""
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('size_gate', ROOT / 'scripts/gate.py')
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)


class SizeGateTests(unittest.TestCase):
    def test_oversized_native_rust_c_and_header_are_rejected(self):
        original = Path.read_text
        for name in ['scripts/native/build.rs', 'scripts/native/owner.c', 'scripts/native/bridge.h',
                     'helpers/guardian/src/main.rs', 'scripts/guardian/protocol.rs',
                     'examples/event_stream_reconnect.rs', 'experiments/native/packed-pages.h']:
            target = ROOT / name
            with self.subTest(path=name):
                def read(path, *args, **kwargs):
                    return 'code\n' * 351 if path == target else original(path, *args, **kwargs)
                with patch.object(Path, 'read_text', read):
                    with self.assertRaisesRegex(SystemExit, 'exceeds 350'):
                        gate.architecture()


if __name__ == '__main__':
    unittest.main()
