"""Failure evidence must survive even when the performance child times out."""
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import MagicMock, patch

SOURCE = Path(__file__).resolve().parents[1] / 'performance.py'
SPEC = importlib.util.spec_from_file_location('performance', SOURCE)
PERFORMANCE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PERFORMANCE)


class PerformanceEvidence(unittest.TestCase):
    def test_timeout_preserves_raw_and_metadata(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / 'evidence'
            child = MagicMock(pid=12345)
            child.communicate.side_effect = [
                subprocess.TimeoutExpired(['cargo'], 600, output=b'partial test output\n'),
                ('partial test output\n', None),
            ]
            with patch('sys.argv', ['performance.py', '--output', str(output)]), \
                    patch.object(PERFORMANCE, 'identity', return_value='stable'), \
                    patch.object(PERFORMANCE.platform, 'platform', return_value='test-platform'), \
                    patch.object(PERFORMANCE.subprocess, 'check_output', return_value='synthetic\n'), \
                    patch.object(PERFORMANCE.subprocess, 'Popen', return_value=child), \
                    patch.object(PERFORMANCE.os, 'killpg') as kill:
                with self.assertRaises(SystemExit) as exited:
                    PERFORMANCE.main()
            self.assertEqual(exited.exception.code, 124)
            kill.assert_called_once()
            self.assertEqual((output / 'raw.txt').read_text(), 'partial test output\n')
            metadata = json.loads((output / 'metadata.json').read_text())
            self.assertTrue(metadata['timed_out'])
            self.assertEqual(metadata['exit_code'], 124)
            self.assertTrue(metadata['source_stable'])

    def test_existing_evidence_is_never_overwritten(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            (output / 'raw.txt').write_text('old failed evidence')
            with patch('sys.argv', ['performance.py', '--output', str(output)]):
                with self.assertRaises(FileExistsError):
                    PERFORMANCE.main()
            self.assertEqual((output / 'raw.txt').read_text(), 'old failed evidence')


if __name__ == '__main__':
    unittest.main()
