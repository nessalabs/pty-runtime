"""Failed resource sampling must preserve both collector and child evidence."""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'release'))
SPEC = importlib.util.spec_from_file_location('release_load', Path(__file__).resolve().parents[1] / 'release/load.py')
load = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(load)
resources = sys.modules[load.census.process_costs.__module__]


class LoadDiagnostics(unittest.TestCase):
    def test_failed_census_retains_child_exit_and_queued_stderr(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / 'fixture'
            binary.write_text('#!' + sys.executable + '\nimport sys\nprint(\'{"event":"checkpoint","phase":"ready"}\', flush=True)\nprint("fixture final diagnostic", file=sys.stderr, flush=True)\nsys.exit(17)\n')
            binary.chmod(0o700)
            destination = root / 'result.jsonl'
            def fail_sample(*args):
                time.sleep(0.1)
                raise ProcessLookupError('fixture sampling failure')
            with patch.object(load.census, 'sample', side_effect=fail_sample):
                self.assertFalse(load.trial(binary, {'seconds': 1, 'warmup': 0, 'active': 0}, destination, True, 1, {}, 5))
            events = [json.loads(line) for line in destination.read_text().splitlines()]
            failure = next(row for row in events if row.get('event') == 'trial_failure')
            self.assertIn('fail_sample', failure['traceback'])
            self.assertEqual(failure['last_event'], 'checkpoint')
            result = next(row for row in events if row.get('event') == 'trial_failure_process')
            self.assertEqual(result['exit_code'], 17)
            self.assertFalse(result['killed_by_driver'])
            self.assertTrue(any(row.get('text') == 'fixture final diagnostic' for row in events))

    def test_failed_checkpoint_acknowledgement_does_not_escape_cleanup(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / 'fixture'
            binary.write_text('#!' + sys.executable + '\nimport sys\nprint(\'{"event":"checkpoint","phase":"ready"}\', flush=True)\nsys.exit(17)\n')
            binary.chmod(0o700)
            destination = root / 'result.jsonl'
            def sample_exited_owner(*args):
                time.sleep(0.1)
                return dict(event='physical_resources', phase='ready')
            with patch.object(load.census, 'sample', side_effect=sample_exited_owner):
                self.assertFalse(load.trial(binary, {'seconds': 1, 'warmup': 0, 'active': 0}, destination, True, 1, {}, 5))
            events = [json.loads(line) for line in destination.read_text().splitlines()]
            self.assertTrue(any(row.get('event') == 'trial_failure' and 'BrokenPipeError' in row['error'] for row in events))
            self.assertTrue(any(row.get('event') == 'trial_failure_process' and row['exit_code'] == 17 for row in events))
            self.assertTrue(any(row.get('event') == 'trial_cleanup_failure' for row in events))

    def test_empty_lsof_failure_identifies_command_and_pids(self):
        result = subprocess.CompletedProcess(['lsof'], 1, '', '')
        with patch.object(resources.platform, 'system', return_value='Darwin'), patch.object(resources.subprocess, 'check_output', return_value='123 100 0:00.01\n'), patch.object(resources.subprocess, 'run', return_value=result):
            with self.assertRaisesRegex(ProcessLookupError, r'lsof.*123.*exit=1'):
                resources.process_costs([123])

    def test_vanished_lsof_process_uses_owner_fallback_with_evidence(self):
        inventory = subprocess.CompletedProcess(['lsof'], 0, 'p123\nf0\n', '')
        threads = subprocess.CompletedProcess(['ps'], 1, '', '')
        with patch.object(load.census, 'members', return_value={123: (1, 'S'), 456: (123, 'S')}), patch.object(resources.platform, 'system', return_value='Darwin'), patch.object(resources.subprocess, 'check_output', side_effect=['123 100 0:00.01\n456 20 0:00.01\n', '123 100 0:00.01\n']), patch.object(resources.subprocess, 'run', side_effect=[inventory, inventory, threads]):
            sample = load.census.sample(123, {456}, 'periodic')
        self.assertEqual([row['pid'] for row in sample['processes']], [123])
        self.assertEqual(sample['unavailable_pids'], [456])
        self.assertIn('lsof', sample['unavailable_reason'])
        self.assertIn('456', sample['unavailable_reason'])

    def test_missing_ps_process_identifies_missing_pid(self):
        result = subprocess.CompletedProcess(['lsof'], 0, 'p123\nf0\np456\nf0\n', '')
        with patch.object(resources.platform, 'system', return_value='Darwin'), patch.object(resources.subprocess, 'check_output', return_value='123 100 0:00.01\n'), patch.object(resources.subprocess, 'run', return_value=result):
            with self.assertRaisesRegex(ProcessLookupError, r'ps.*456'):
                resources.process_costs([123, 456])


if __name__ == '__main__':
    unittest.main()
