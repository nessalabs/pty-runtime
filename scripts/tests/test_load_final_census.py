"""The final acknowledged census ends sampling, but not exit validation."""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'release'))
SPEC = importlib.util.spec_from_file_location('release_load_final', Path(__file__).resolve().parents[1] / 'release/load.py')
load = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(load)


class FinalCensus(unittest.TestCase):
    def run_fixture(self, final_phase='closed', complete=True, exit_code=0, tree_processes=1, zombies=(), advance_clock=True):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / 'fixture'
            binary.write_text('#!' + sys.executable + '\n' + f'''
import json, sys
for phase in ['measurement_start', 'measurement_end', {final_phase!r}]:
    print(json.dumps(dict(event='checkpoint', phase=phase)), flush=True)
    input()
if {complete!r}:
    print(json.dumps(dict(event='complete')), flush=True)
sys.exit({exit_code})
''')
            binary.chmod(0o700)
            destination = root / 'result.jsonl'
            now = [0.0]
            samples = []
            children = []
            forced_poll = [False]
            popen = subprocess.Popen

            def capture(*args, **kwargs):
                child = popen(*args, **kwargs)
                children.append(child)
                original_poll = child.poll

                def poll():
                    status = original_poll()
                    if now[0] == 10 and not forced_poll[0]:
                        forced_poll[0] = True
                        # Deterministically put normal owner exit between the live
                        # poll observation and the driver's next census call.
                        child.wait(timeout=3)
                        return None
                    return status

                child.poll = poll
                return child

            def sample(owner, workloads, phase):
                samples.append(phase)
                if phase == 'periodic':
                    raise ProcessLookupError('owner exited after poll')
                if advance_clock and phase == final_phase and tree_processes == 1 and not zombies:
                    now[0] = 10.0
                return dict(event='physical_resources', phase=phase,
                            tree_processes=tree_processes, zombies=list(zombies))

            with patch.object(load.subprocess, 'Popen', side_effect=capture), patch.object(load.time, 'monotonic', side_effect=lambda: now[0]), patch.object(load.census, 'sample', side_effect=sample), patch.object(load.census, 'cpu_delta', return_value=dict(event='cpu_interval')):
                passed = load.trial(binary, dict(seconds=1, warmup=0, active=0, mode='resources'), destination, True, 1, {}, 5)
            events = [json.loads(line) for line in destination.read_text().splitlines()]
            self.assertTrue(children[0].stdin.closed)
            self.assertTrue(children[0].stdout.closed)
            return passed, samples, events

    def test_closed_census_stops_periodic_sampling_before_normal_exit(self):
        passed, samples, events = self.run_fixture()
        self.assertTrue(passed, events)
        self.assertEqual(samples, ['measurement_start', 'measurement_end', 'closed'])
        self.assertTrue(any(row.get('event') == 'complete' for row in events))
        self.assertEqual(events[-1]['event'], 'trial_result')

    def test_owner_disappearance_before_closed_remains_failure(self):
        passed, samples, events = self.run_fixture(final_phase='ready')
        self.assertFalse(passed)
        self.assertEqual(samples[-1], 'periodic')
        failure = next(row for row in events if row.get('event') == 'trial_failure')
        self.assertIn('ProcessLookupError', failure['error'])
        self.assertEqual(failure['last_checkpoint'], 'ready')

    def test_closed_still_requires_complete_and_successful_exit(self):
        for complete, exit_code in [(False, 0), (True, 17)]:
            with self.subTest(complete=complete, exit_code=exit_code):
                passed, _, events = self.run_fixture(complete=complete, exit_code=exit_code)
                self.assertFalse(passed)
                self.assertFalse(any(row.get('event') == 'trial_result' for row in events))

    def test_clean_exit_without_closed_census_is_rejected(self):
        passed, samples, events = self.run_fixture(final_phase='ready', advance_clock=False)
        self.assertEqual(samples, ['measurement_start', 'measurement_end', 'ready'])
        self.assertTrue(any(row.get('event') == 'complete' for row in events))
        self.assertFalse(passed, 'clean exit must not replace final cleanup census')
        self.assertFalse(any(row.get('event') == 'trial_result' for row in events))

    def test_closed_census_still_rejects_descendants_and_zombies(self):
        for tree_processes, zombies in [(2, ()), (1, (123,))]:
            with self.subTest(tree_processes=tree_processes, zombies=zombies):
                passed, _, events = self.run_fixture(tree_processes=tree_processes, zombies=zombies)
                self.assertFalse(passed)
                self.assertTrue(any(row.get('event') == 'trial_failure' and 'AssertionError' in row['error'] for row in events))


if __name__ == '__main__':
    unittest.main()
