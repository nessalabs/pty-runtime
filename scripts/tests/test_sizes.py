"""Architecture gate: which checks block, and which only report.

File size is an alarm worth looking at, but a coherent file is better than one
chopped up to satisfy a threshold, so an oversized file is reported and the gate
continues. Dependency direction is the opposite: it is the rule the layering
depends on and it must fail closed. These tests pin both, because softening one
must not quietly soften the other.
"""
import importlib.util
import io
import json
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


def _metadata_with(extra_dependency, on_package):
    """Real workspace metadata with one forbidden edge spliced in."""
    import subprocess
    raw = json.loads(subprocess.check_output(
        ['cargo', 'metadata', '--locked', '--no-deps', '--format-version', '1'], cwd=ROOT))
    for package in raw['packages']:
        if package['name'] == on_package:
            package['dependencies'].append(extra_dependency)
    return json.dumps(raw).encode()


class DependencyGateTests(unittest.TestCase):
    """Dependency direction must fail closed, unlike file size."""

    def _assert_rejected(self, package, dependency, expected):
        spliced = _metadata_with(dependency, package)
        with patch('subprocess.check_output', return_value=spliced):
            with self.assertRaisesRegex(SystemExit, expected):
                gate.architecture()

    def test_domain_cannot_depend_on_application(self):
        self._assert_rejected(
            'pty-runtime-domain',
            {'name': 'pty-runtime-application', 'path': str(ROOT / 'crates/application')},
            'Forbidden dependency in pty-runtime-domain',
        )

    def test_application_cannot_depend_on_infrastructure(self):
        self._assert_rejected(
            'pty-runtime-application',
            {'name': 'pty-runtime-infrastructure', 'path': str(ROOT / 'crates/infrastructure')},
            'Forbidden dependency in pty-runtime-application',
        )

    def test_infrastructure_cannot_depend_on_the_facade(self):
        self._assert_rejected(
            'pty-runtime-infrastructure',
            {'name': 'pty-runtime', 'path': str(ROOT)},
            'Reversed workspace dependency',
        )

    def test_unmodified_workspace_passes(self):
        with redirect_stdout(io.StringIO()):
            gate.architecture()


if __name__ == '__main__':
    unittest.main()
