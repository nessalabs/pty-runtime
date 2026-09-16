"""A slope check is only useful if it is hard to fool in both directions.

ADR 0002's soak criterion is about accumulation, so the two ways to be wrong are
missing a real leak and inventing one from noise. These pin both.
"""
import importlib.util
import json
from pathlib import Path
import random
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location(
    'release_accumulation', Path(__file__).resolve().parents[1] / 'release/accumulation.py')
accumulation = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(accumulation)


def series(values, start=0.0, step=5.0, metric='fds'):
    return [{'monotonic_seconds': start + index * step, metric: value,
             'rss_bytes': 1000, 'threads': 4, 'tree_processes': 2}
            for index, value in enumerate(values)]


class Accumulation(unittest.TestCase):
    def test_a_steady_leak_is_found(self):
        # One descriptor every other sample: 360/hour at a 5s interval.
        rows = series([10 + index // 2 for index in range(40)])
        result = accumulation.assess(rows, 0, 'fds', accumulation.FLOORS['fds'])
        self.assertTrue(result['accumulating'])
        self.assertGreater(result['growth_over_window'], 5)

    def test_a_flat_series_with_noise_is_not_called_a_leak(self):
        random.seed(7)
        rows = series([10 + random.choice([-1, 0, 1]) for _ in range(40)])
        result = accumulation.assess(rows, 0, 'fds', accumulation.FLOORS['fds'])
        self.assertFalse(result['accumulating'])
        self.assertEqual(result['verdict'], 'flat')

    def test_a_certain_but_tiny_slope_is_not_a_finding(self):
        """Statistical certainty is not the same as mattering."""
        rows = series([10 + index * 1e-9 for index in range(40)], metric='fds')
        result = accumulation.assess(rows, 0, 'fds', accumulation.FLOORS['fds'])
        self.assertFalse(result['large_enough_to_matter'])
        self.assertFalse(result['accumulating'])

    def test_a_big_slope_over_too_few_samples_is_undecided_not_confirmed(self):
        rows = series([10, 40, 90])
        result = accumulation.assess(rows, 0, 'fds', accumulation.FLOORS['fds'])
        self.assertIsNone(result['accumulating'])
        self.assertEqual(result['verdict'], 'insufficient data')

    def test_warmup_is_excluded_so_a_startup_ramp_is_not_a_leak(self):
        # Climbs hard for 60s, then holds. That is startup, not accumulation.
        rows = series([10 + index * 5 for index in range(12)] + [65] * 28)
        included = accumulation.assess(rows, 60, 'fds', accumulation.FLOORS['fds'])
        self.assertFalse(included['accumulating'], 'the ramp is before the warmup mark')
        ignored = accumulation.assess(rows, 0, 'fds', accumulation.FLOORS['fds'])
        self.assertTrue(ignored['accumulating'], 'without a warmup the ramp dominates')

    def test_a_decreasing_series_is_not_reported_as_accumulating(self):
        rows = series([100 - index for index in range(40)])
        result = accumulation.assess(rows, 0, 'fds', accumulation.FLOORS['fds'])
        self.assertFalse(result['accumulating'])
        self.assertEqual(result['verdict'], 'decreasing')

    def test_a_missing_metric_is_undecided_rather_than_flat(self):
        rows = [{'monotonic_seconds': float(n), 'fds': None} for n in range(40)]
        result = accumulation.assess(rows, 0, 'fds', accumulation.FLOORS['fds'])
        self.assertIsNone(result['accumulating'])

    def test_an_hour_projected_from_a_minute_is_marked_untrustworthy(self):
        """Sixty seconds of RSS wobble became 16-35 MiB/hour on real data."""
        rows = series([1000] * 13, step=5.0)
        result = accumulation.assess(rows, 0, 'fds', accumulation.FLOORS['fds'])
        self.assertGreater(result['extrapolation_factor'], 10)
        self.assertFalse(result['per_hour_is_trustworthy'])

    def test_memory_is_judged_relative_to_its_own_size(self):
        """0.3 MiB of drift on a 443 MiB process is not a leak."""
        rows = [{'monotonic_seconds': n * 5.0, 'rss_bytes': 443_000_000 + n * 25_000}
                for n in range(13)]
        result = accumulation.assess(rows, 0, 'rss_bytes', accumulation.FLOORS['rss_bytes'])
        self.assertFalse(result['accumulating'])
        self.assertEqual(result['verdict'], 'moving but too little to matter')

    def test_memory_growing_by_a_real_fraction_is_still_caught(self):
        rows = [{'monotonic_seconds': n * 5.0, 'rss_bytes': 443_000_000 + n * 2_000_000}
                for n in range(13)]
        result = accumulation.assess(rows, 0, 'rss_bytes', accumulation.FLOORS['rss_bytes'])
        self.assertTrue(result['accumulating'])

    def test_turnover_that_keeps_the_count_flat_cannot_hide_a_leak(self):
        """The hole a count-only comparability check leaves open.

        A costly process leaves and a cheap one arrives, so `measured_processes`
        never moves and the totals wander for a reason that is not the resource.
        The cohort — the processes present in every census — is climbing.
        """
        rows = []
        for index in range(40):
            rows.append({'monotonic_seconds': index * 5.0,
                         'measured_processes': 2,
                         # Totals are flat: the cohort's growth is cancelled by a
                         # different process being measured each time.
                         'fds': 100,
                         'cohort_processes': 1,
                         'cohort_fds': 10 + index // 2})
        result = accumulation.assess(rows, 0, 'fds', accumulation.FLOORS['fds'])
        self.assertTrue(result['accumulating'])
        self.assertEqual(result['basis'], 'stable_cohort')
        self.assertEqual(result['bases']['all_measured']['verdict'], 'flat',
                         'the whole-tree totals really do look flat')

    def test_a_leak_in_the_churn_is_still_caught_by_the_whole_tree_totals(self):
        """The cohort cannot see a leak that lives outside it, so both are fitted."""
        rows = [{'monotonic_seconds': index * 5.0, 'measured_processes': 2,
                 'fds': 100 + index // 2, 'cohort_processes': 1, 'cohort_fds': 10}
                for index in range(40)]
        result = accumulation.assess(rows, 0, 'fds', accumulation.FLOORS['fds'])
        self.assertTrue(result['accumulating'])
        self.assertEqual(result['bases']['stable_cohort']['verdict'], 'flat')

    def test_an_artifact_without_a_cohort_still_uses_the_totals(self):
        """Artifacts written before retention version 4 carry no cohort series."""
        rows = series([10 + index // 2 for index in range(40)])
        result = accumulation.assess(rows, 0, 'fds', accumulation.FLOORS['fds'])
        self.assertEqual(result['basis'], 'all_measured')
        self.assertTrue(result['accumulating'])

    def test_accumulating_children_are_the_finding_not_a_reason_to_refuse(self):
        """The leak ADR 0002 names, which the comparability gate used to swallow.

        Persistent children raise the measured count and the tree count
        together, so gating `tree_processes` on population movement made its own
        growth disqualify it — and with no cohort analogue for a count, that
        left no basis at all and a verdict of `null`.
        """
        rows = [{'monotonic_seconds': index * 5.0,
                 'tree_processes': 10 + index // 4,
                 'measured_processes': 10 + index // 4}
                for index in range(40)]
        result = accumulation.assess(rows, 0, 'tree_processes',
                                     accumulation.FLOORS['tree_processes'])
        self.assertTrue(result['accumulating'], result['verdict'])
        self.assertEqual(result['verdict'], 'accumulating')
        self.assertGreater(result['growth_over_window'], 1)

    def test_a_moving_population_still_disqualifies_sums_taken_over_it(self):
        """The gate is right for totals; it was only wrong for the count itself."""
        rows = [{'monotonic_seconds': index * 5.0,
                 'fds': 100, 'measured_processes': 10 + index // 4}
                for index in range(40)]
        result = accumulation.assess(rows, 0, 'fds', accumulation.FLOORS['fds'])
        self.assertIsNone(result['accumulating'])
        self.assertIn('population moved', result['verdict'])

    def test_a_series_that_alternates_is_undecided_rather_than_leaking(self):
        """The real 55-minute soak, in miniature.

        A metric that visits two values as a transient probe comes and goes
        hands a confident slope to any fit whose window catches one value more
        often at one end. That is a duty cycle shifting, not a resource
        accumulating, and the line explained 2.8% of the variance on real data.
        """
        # Alternates 1864/1873 with the high value growing from 30% of samples
        # to 70%, over the sample count a 55-minute soak produces.
        rows = [{'monotonic_seconds': index * 5.0,
                 'fds': 1873 if (index % 10) < 3 + 4 * index / 600 else 1864}
                for index in range(600)]
        result = accumulation.assess(rows, 0, 'fds', accumulation.FLOORS['fds'])
        self.assertGreater(result['significance'], accumulation.SIGNIFICANCE,
                           'the slope is confident, which is the trap')
        self.assertTrue(result['large_enough_to_matter'])
        self.assertIsNone(result['accumulating'], result['verdict'])
        self.assertIn('a line does not describe', result['verdict'])
        self.assertLess(result['variance_explained'], accumulation.LINEARITY)

    def test_a_clean_leak_still_reads_as_one(self):
        """The linearity guard must not blunt the thing the tool is for."""
        rows = series([10 + index // 2 for index in range(40)])
        result = accumulation.assess(rows, 0, 'fds', accumulation.FLOORS['fds'])
        self.assertTrue(result['accumulating'])
        self.assertGreater(result['variance_explained'], 0.9)

    def test_an_undecided_metric_never_exits_as_a_pass(self):
        import sys
        rows = [{'monotonic_seconds': index * 5.0,
                 'fds': 1873 if (index % 10) < 3 + 4 * index / 600 else 1864}
                for index in range(600)]
        artifact = {'trials': [{'case': 'attached', 'trial': 1, 'resource_series': rows}]}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'a.json'
            path.write_text(json.dumps(artifact))
            sys.argv = ['accumulation', '--artifact', str(path), '--warmup-seconds', '0']
            with self.assertRaises(SystemExit) as raised:
                accumulation.main()
            self.assertEqual(raised.exception.code, 2)

    def test_no_evidence_does_not_exit_as_though_nothing_accumulated(self):
        """A gate must not read an empty artifact as a flat resource profile."""
        import sys
        for trials in ([], [{'case': 'idle-64', 'trial': 1, 'resource_series': series([10, 11])}]):
            with tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / 'a.json'
                path.write_text(json.dumps({'trials': trials}))
                sys.argv = ['accumulation', '--artifact', str(path), '--warmup-seconds', '0']
                with self.assertRaises(SystemExit) as raised:
                    accumulation.main()
                self.assertEqual(raised.exception.code, 2, 'insufficient evidence is not a pass')

    def test_a_decided_flat_run_still_exits_zero(self):
        import sys
        artifact = {'trials': [{'case': 'idle-64', 'trial': 1,
                                'resource_series': series([10] * 40)}]}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'a.json'
            path.write_text(json.dumps(artifact))
            sys.argv = ['accumulation', '--artifact', str(path), '--warmup-seconds', '0']
            with self.assertRaises(SystemExit) as raised:
                accumulation.main()
            self.assertEqual(raised.exception.code, 0)

    def test_the_exit_code_reports_a_leak_so_a_run_can_gate_on_it(self):
        artifact = {'trials': [
            {'case': 'idle-64', 'trial': 1,
             'resource_series': series([10 + index // 2 for index in range(40)])}]}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'a.json'
            path.write_text(json.dumps(artifact))
            import sys
            sys.argv = ['accumulation', '--artifact', str(path), '--warmup-seconds', '0']
            with self.assertRaises(SystemExit) as raised:
                accumulation.main()
            self.assertEqual(raised.exception.code, 1)


if __name__ == '__main__':
    unittest.main()
