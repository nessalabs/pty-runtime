"""The artifact must retain what a later reader is asked to audit.

Experiment 0005 kept only a final census and a filtered event list, so the
resource series behind six of its cases could not be checked afterwards and the
claim had to be withdrawn. These tests pin the retention contract so that
cannot happen by omission again.
"""
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location(
    'release_archive', Path(__file__).resolve().parents[1] / 'release/archive.py')
archive = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(archive)


def write(root, name, events):
    (root / name).write_text('\n'.join(json.dumps(event) for event in events) + '\n')


def identity(**extra):
    return {'event': 'identity', 'platform': 'Linux', 'source_head': 'abc',
            'source_inventory': [{'path': f'f{n}', 'sha256': 'x'} for n in range(300)],
            'configuration': {'sessions': 1}, 'repeat': 1, **extra}


def census(at, phase, processes, **extra):
    return {'event': 'physical_resources', 'phase': phase, 'monotonic_seconds': at,
            'processes': processes, 'tree_processes': len(processes),
            'zombies': [], 'unavailable_pids': [], **extra}


def proc(pid, rss, fds, threads=2, cpu=1.0):
    return {'pid': pid, 'rss_bytes': rss, 'fds': fds, 'threads': threads,
            'cpu_seconds': cpu, 'pss_bytes': None}


class Retention(unittest.TestCase):
    def build(self, case, events):
        root = Path(self.directory)
        write(root, 'trial.jsonl', events)
        (root / 'summary.json').write_text(json.dumps(
            {'results': [{'case': case, 'trial': 1, 'passed': True, 'path': 'trial.jsonl'}]}))
        out = root / 'artifact.json'
        sys.argv = ['archive', '--input', str(root), '--output', str(out)]
        archive.main()
        return json.loads(out.read_text())

    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.directory = self._tmp.name
        self.addCleanup(self._tmp.cleanup)

    def test_the_resource_series_survives_so_accumulation_stays_visible(self):
        events = [identity()] + [
            census(float(at), 'periodic', [proc(1, 1000 + at, 10 + at)]) for at in range(5)]
        artifact = self.build('attached', events)
        series = artifact['trials'][0]['resource_series']
        self.assertEqual([row['rss_bytes'] for row in series], [1000, 1001, 1002, 1003, 1004])
        self.assertEqual([row['fds'] for row in series], [10, 11, 12, 13, 14])
        self.assertEqual([row['monotonic_seconds'] for row in series], [0.0, 1.0, 2.0, 3.0, 4.0])

    def test_resource_cases_keep_rows_at_steady_state_and_at_quiescence(self):
        rows = [proc(1, 100, 4), proc(2, 200, 5)]
        artifact = self.build('resources-projected-32', [
            identity(),
            census(1.0, 'measurement_end', rows),
            census(2.0, 'closed', [proc(1, 100, 4)])])
        kept = artifact['trials'][0]['census_full']
        self.assertEqual([entry['phase'] for entry in kept], ['measurement_end', 'closed'])
        self.assertEqual([row['pid'] for row in kept[0]['processes']], [1, 2],
                         'per-process scaling is the whole point of a resources- case')

    def test_periodic_rows_are_dropped_but_their_totals_are_not(self):
        """Nineteen near-identical censuses were 90% of a real artifact."""
        events = [identity()] + [
            census(float(at), 'periodic', [proc(pid, 100, 4) for pid in range(30)])
            for at in range(6)]
        trial = self.build('resources-projected-32', events)['trials'][0]
        self.assertEqual(trial['census_full'], [], 'periodic rows repeat the same answer')
        self.assertEqual(len(trial['resource_series']), 6, 'accumulation must still be visible')
        self.assertEqual(trial['resource_series'][0]['measured_processes'], 30)

    def test_other_cases_keep_process_rows_only_for_the_closing_census(self):
        artifact = self.build('attached', [
            identity(),
            census(1.0, 'periodic', [proc(1, 100, 4)]),
            census(2.0, 'closed', [proc(1, 100, 4)])])
        kept = artifact['trials'][0]['census_full']
        self.assertEqual([entry['phase'] for entry in kept], ['closed'],
                         'cleanup needs the rows; every periodic sample does not')

    def test_budget_peaks_replace_the_records_without_losing_the_peak(self):
        events = [identity()] + [
            {'event': 'budget', 'phase': 'under_load', 'name': 'views', 'used': used, 'limit': 64}
            for used in (1, 63, 7)]
        peaks = self.build('attached', events)['trials'][0]['budget_peaks']
        self.assertEqual(peaks['views'], {'used_peak': 63, 'limit': 64, 'samples': 3})

    def test_the_latency_distribution_is_trimmed_but_not_lost(self):
        buckets = [0, 0, 5, 9, 0, 0, 0, 2]
        artifact = self.build('attached', [identity(), {
            'event': 'latency', 'boundary': 'ProjectedOutput', 'successes': 16, 'failures': 0,
            'unavailable': 0, 'p50_us': 300, 'p95_us': 400, 'p99_us': 700, 'max_us': 700,
            'bucket_width_us': 100, 'buckets': buckets}])
        row = artifact['trials'][0]['latency'][0]
        self.assertEqual(row['distribution'], {'first_bucket': 2, 'counts': [5, 9, 0, 0, 0, 2]})
        self.assertEqual(sum(row['distribution']['counts']), 16, 'every sample must still be there')
        self.assertEqual(row['overflow_bucket_samples'], 2)
        self.assertEqual(row['histogram_ceiling_us'], 700)

    def test_identity_is_hoisted_once_and_divergence_is_not_hidden(self):
        artifact = self.build('attached', [identity()])
        self.assertIn('source_inventory', artifact['identity'])
        self.assertNotIn('identity', artifact['trials'][0])
        self.assertEqual(artifact['identity_divergent_trials'], [])
        self.assertEqual(artifact['trials'][0]['run']['configuration'], {'sessions': 1})

    def test_a_failed_trial_keeps_its_failure_and_stderr(self):
        artifact = self.build('attached', [
            identity(),
            {'event': 'stderr', 'text': 'Error: Process(Io)'},
            {'event': 'trial_failure', 'error': 'AssertionError', 'last_checkpoint': 'runtime'}])
        trial = artifact['trials'][0]
        self.assertEqual(trial['failure']['last_checkpoint'], 'runtime')
        self.assertEqual(trial['stderr'], ['Error: Process(Io)'])


if __name__ == '__main__':
    unittest.main()
