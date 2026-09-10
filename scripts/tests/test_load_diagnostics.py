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

    def test_darwin_cpu_minutes_continue_past_one_hour(self):
        inventory = subprocess.CompletedProcess(['lsof'], 0, 'p123\nf0\n', '')
        for elapsed, expected in [('59:59.99', 3599.99), ('60:00.00', 3600.0), ('1295:17.02', 77717.02)]:
            with self.subTest(elapsed=elapsed), patch.object(resources.platform, 'system', return_value='Darwin'), patch.object(resources.subprocess, 'check_output', return_value=f'123 100 {elapsed}\n'), patch.object(resources.subprocess, 'run', return_value=inventory):
                self.assertAlmostEqual(resources.process_costs([123])[0]['cpu_seconds'], expected)

    def test_unavailable_idle_cpu_cannot_pass_target_but_is_not_correctness_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / 'fixture'
            binary.write_text('#!' + sys.executable + '\nimport json\nfor phase in ["measurement_start", "measurement_end", ' + repr(load.FINAL_CENSUS_PHASE) + ']:\n print(json.dumps(dict(event="checkpoint", phase=phase)), flush=True)\n input()\nprint(\'{"event":"complete"}\', flush=True)\n')
            binary.chmod(0o700)
            destination = root / 'result.jsonl'
            with patch.object(load.census, 'sample', side_effect=lambda owner, workloads, phase: dict(event='physical_resources', phase=phase, tree_processes=1, zombies=[])), patch.object(load.census, 'cpu_delta', return_value=dict(event='cpu_interval', categories=dict(owner=dict(core_percent=None)))):
                self.assertTrue(load.trial(binary, {'seconds': 1, 'warmup': 0, 'active': 0, 'mode': 'idle'}, destination, True, 1, {}, 5))
            events = [json.loads(line) for line in destination.read_text().splitlines()]
            target = next(row for row in events if row.get('event') == 'idle_cpu_target')
            self.assertIsNone(target['measured_core_percent'])
            self.assertIsNone(target['passed'])
            self.assertFalse(target['acceptance_duration'])

    def test_success_closes_owned_pipes_after_final_output_and_exit(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / 'fixture'
            binary.write_text('#!' + sys.executable + '\nimport json,sys\nfor phase in ["measurement_start", "measurement_end", ' + repr(load.FINAL_CENSUS_PHASE) + ']:\n print(json.dumps(dict(event="checkpoint", phase=phase)), flush=True)\n input()\nprint(\'{"event":"complete"}\', flush=True)\nprint("final fixture stderr", file=sys.stderr, flush=True)\n')
            binary.chmod(0o700)
            destination = root / 'result.jsonl'
            children = []
            popen = subprocess.Popen
            def capture(*args, **kwargs):
                child = popen(*args, **kwargs)
                children.append(child)
                return child
            with patch.object(load.subprocess, 'Popen', side_effect=capture), patch.object(load.census, 'sample', side_effect=lambda owner, workloads, phase: dict(event='physical_resources', phase=phase, tree_processes=1, zombies=[])), patch.object(load.census, 'cpu_delta', return_value=dict(event='cpu_interval', categories=dict(owner=dict(core_percent=0.0)))):
                self.assertTrue(load.trial(binary, {'seconds': 1, 'warmup': 0, 'active': 0, 'mode': 'idle'}, destination, True, 1, {}, 5))
            child, = children
            self.assertEqual(child.returncode, 0)
            self.assertTrue(child.stdin.closed, 'owned stdin remains open after successful trial')
            self.assertTrue(child.stdout.closed, 'owned stdout remains open after successful trial')
            events = [json.loads(line) for line in destination.read_text().splitlines()]
            self.assertTrue(any(row.get('text') == 'final fixture stderr' for row in events))
            self.assertEqual(events[-1]['event'], 'trial_result')

    def test_missing_ps_process_identifies_missing_pid(self):
        result = subprocess.CompletedProcess(['lsof'], 0, 'p123\nf0\np456\nf0\n', '')
        with patch.object(resources.platform, 'system', return_value='Darwin'), patch.object(resources.subprocess, 'check_output', return_value='123 100 0:00.01\n'), patch.object(resources.subprocess, 'run', return_value=result):
            with self.assertRaisesRegex(ProcessLookupError, r'ps.*456'):
                resources.process_costs([123, 456])


if __name__ == '__main__':
    unittest.main()
