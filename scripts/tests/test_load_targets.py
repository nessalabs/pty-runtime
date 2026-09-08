"""Summary performance targets stay distinct from correctness and missing evidence."""
import json
import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'release'))
from load_support import reporting
SPEC = importlib.util.spec_from_file_location('load_targets_driver', Path(__file__).resolve().parents[1] / 'release/load.py')
driver = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(driver)


class TargetRollups(unittest.TestCase):
    def result(self, config, events):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'trial.jsonl'
            path.write_text(''.join(json.dumps(row) + '\n' for row in events))
            return reporting.trial_targets(path, config)

    def test_unavailable_measurement_is_distinct_from_observed_failure(self):
        for observed, failures, unavailable, expected in [
            (None, 0, 0, None), (10, 0, 1, None), (30, 0, 0, False),
            (None, 1, 0, False), (30, 0, 1, False), (10, 0, 0, True),
        ]:
            with self.subTest(observed=observed, failures=failures, unavailable=unavailable):
                self.assertIs(reporting.measurement_verdict(observed, 20, failures, unavailable), expected)

    def test_explicit_failure_preserves_missing_boundary_evidence(self):
        events = [dict(event='latency_target', boundary=b, passed=True, measurement_complete=True) for b in reporting.LATENCY_BOUNDARIES]
        events[0].update(passed=False, observed_p99_us=None, failures=1, measurement_complete=False)
        result = self.result(dict(active=1, raw=False, mode='attached'), events)
        self.assertFalse(result['latency_targets_passed'])
        self.assertEqual(result['latency_targets_missing'], ['InputDispatch'])
        summary = reporting.rollup([result])
        self.assertEqual(summary['latency']['failed_trials'], 1)
        self.assertEqual(summary['latency']['incomplete_trials'], 1)

    def test_present_unmeasured_targets_roll_up_as_unmeasured(self):
        events = [dict(event='latency_target', boundary=b, observed_p99_us=None, passed=None, measurement_complete=False) for b in reporting.LATENCY_BOUNDARIES]
        result = self.result(dict(active=1, raw=False, mode='attached'), events)
        summary = reporting.rollup([result])
        self.assertEqual(summary['latency']['unmeasured_trials'], 1)
        self.assertEqual(summary['latency']['failed_trials'], 0)
        idle = self.result(dict(active=0, raw=False, mode='idle'), [dict(event='idle_cpu_target', passed=None, measured_core_percent=None, measurement_complete=False)])
        self.assertEqual(reporting.rollup([idle])['idle_cpu']['unmeasured_trials'], 1)

    def test_completed_trial_missed_latency_is_visible_in_rollup(self):
        events = [dict(event='latency_target', boundary=boundary, passed=boundary != 'InputDispatch') for boundary in reporting.LATENCY_BOUNDARIES]
        result = self.result(dict(active=1, raw=False, mode='attached'), events)
        summary = reporting.rollup([dict(passed=True, **result)])
        self.assertFalse(summary['latency']['all_applicable_passed'])
        self.assertEqual(summary['latency']['failed_trials'], 1)
        self.assertIsNone(summary['idle_cpu']['all_applicable_passed'])

    def test_summary_keeps_correctness_pass_when_latency_target_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / 'binary'
            binary.write_bytes(b'fixture image')
            output = root / 'output'
            config = dict(active=1, raw=False, mode='attached')
            def run_trial(binary, config, destination, *args):
                destination.write_text(''.join(json.dumps(dict(event='latency_target', boundary=b, passed=b != 'InputDispatch')) + '\n' for b in reporting.LATENCY_BOUNDARIES))
                return True
            with patch.object(sys, 'argv', ['load.py', '--binary', str(binary), '--output', str(output), '--repeats', '1']), patch.object(driver.matrix, 'cases', return_value={'attached': config}), patch.object(driver.identity, 'identify', return_value={}), patch.object(driver, 'trial', side_effect=run_trial):
                driver.main()
            summary = json.loads((output / 'summary.json').read_text())
            self.assertTrue(summary['all_trials_passed'])
            self.assertEqual(summary['all_trials_passed_scope'], 'execution_and_correctness_accounting_only')
            self.assertFalse(summary['target_rollup']['latency']['all_applicable_passed'])
            self.assertFalse(summary['results'][0]['latency_targets_passed'])

    def test_missing_failed_trial_measurements_are_not_a_pass(self):
        measured = self.result(dict(active=1, raw=True, mode='attached'), [dict(event='latency_target', boundary=b, passed=True) for b in reporting.LATENCY_BOUNDARIES if b != 'ProjectedOutput'])
        missing = self.result(dict(active=1, raw=True, mode='attached'), [])
        summary = reporting.rollup([measured, missing])
        self.assertEqual(summary['latency']['passed_trials'], 1)
        self.assertEqual(summary['latency']['unmeasured_trials'], 1)
        self.assertIsNone(summary['latency']['all_applicable_passed'])

    def test_idle_target_failure_and_smoke_duration_are_separate(self):
        result = self.result(dict(active=0, raw=False, mode='idle'), [dict(event='idle_cpu_target', passed=False, acceptance_duration=False)])
        summary = reporting.rollup([result])
        self.assertFalse(summary['idle_cpu']['all_applicable_passed'])
        self.assertEqual(summary['idle_cpu']['acceptance_duration_trials'], 0)
        self.assertEqual(summary['latency']['applicable_trials'], 0)


if __name__ == '__main__':
    unittest.main()
