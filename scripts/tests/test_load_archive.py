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
            'configuration': {'sessions': 1, 'active': 0, 'mode': 'attached', 'raw': False},
            'repeat': 1, **extra}


def census(at, phase, processes, **extra):
    return {'event': 'physical_resources', 'phase': phase, 'monotonic_seconds': at,
            'processes': processes, 'tree_processes': len(processes),
            'zombies': [], 'unavailable_pids': [], **extra}


def proc(pid, rss, fds, threads=2, cpu=1.0):
    return {'pid': pid, 'rss_bytes': rss, 'fds': fds, 'threads': threads,
            'cpu_seconds': cpu, 'pss_bytes': None}


class Retention(unittest.TestCase):
    def build(self, case, events, terminal=True, passed=None):
        root = Path(self.directory)
        # The archiver refuses a trial with no terminal record, so supply one
        # unless the test is about its absence.
        kinds = {event.get('event') for event in events}
        if terminal and not kinds & {'trial_result', 'trial_failure'}:
            events = events + [{'event': 'trial_result', 'passed': True}]
        write(root, 'trial.jsonl', events)
        if passed is None:
            # The summary must agree with the file; the archiver now checks.
            passed = not any(event.get('event') == 'trial_failure' for event in events)
        (root / 'summary.json').write_text(json.dumps(
            {'results': [{'case': case, 'trial': 1, 'passed': passed, 'path': 'trial.jsonl'}]}))
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
        self.assertEqual(peaks['views'], {'used_peak': 63, 'limit': 64, 'samples': 3,
                                          'final_used': 7, 'final_phase': 'under_load'})

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
        self.assertEqual(artifact['trials'][0]['run']['configuration']['sessions'], 1)

    def test_a_missing_trial_file_fails_rather_than_shrinking_the_evidence(self):
        """An artifact must not look complete while holding fewer trials."""
        root = Path(self.directory)
        (root / 'summary.json').write_text(json.dumps(
            {'results': [{'case': 'attached', 'trial': 1, 'passed': True, 'path': 'gone.jsonl'}]}))
        sys.argv = ['archive', '--input', str(root), '--output', str(root / 'a.json')]
        with self.assertRaises(SystemExit):
            archive.main()

    def test_an_unreadable_line_fails_rather_than_being_dropped(self):
        root = Path(self.directory)
        (root / 'trial.jsonl').write_text(json.dumps(identity()) + '\nnot json at all\n')
        (root / 'summary.json').write_text(json.dumps(
            {'results': [{'case': 'attached', 'trial': 1, 'passed': True, 'path': 'trial.jsonl'}]}))
        sys.argv = ['archive', '--input', str(root), '--output', str(root / 'a.json')]
        with self.assertRaises(ValueError):
            archive.main()

    def test_a_trial_without_an_identity_is_refused(self):
        """Measurements that cannot be attributed to a revision are not evidence."""
        root = Path(self.directory)
        write(root, 'trial.jsonl', [{'event': 'trial_result', 'passed': True}])
        (root / 'summary.json').write_text(json.dumps(
            {'results': [{'case': 'attached', 'trial': 1, 'passed': True, 'path': 'trial.jsonl'}]}))
        sys.argv = ['archive', '--input', str(root), '--output', str(root / 'a.json')]
        with self.assertRaises(SystemExit):
            archive.main()

    def test_a_divergent_identity_names_the_trial_it_came_from(self):
        root = Path(self.directory)
        done = {'event': 'trial_result', 'passed': True}
        write(root, 'a.jsonl', [identity(), done])
        write(root, 'b.jsonl', [identity(source_head='different'), done])
        (root / 'summary.json').write_text(json.dumps({'results': [
            {'case': 'attached', 'trial': 1, 'passed': True, 'path': 'a.jsonl'},
            {'case': 'attached', 'trial': 2, 'passed': True, 'path': 'b.jsonl'}]}))
        out = root / 'a.json'
        sys.argv = ['archive', '--input', str(root), '--output', str(out)]
        archive.main()
        divergent = json.loads(out.read_text())['identity_divergent_trials']
        self.assertEqual(len(divergent), 1)
        self.assertEqual(divergent[0]['trial'], 2)
        self.assertEqual(divergent[0]['file'], 'b.jsonl')
        self.assertEqual(divergent[0]['identity']['source_head'], 'different')

    def test_proportional_memory_survives_in_the_series(self):
        """Per-process rows are dropped for periodic samples; PSS lives only here."""
        rows = [{'pid': 1, 'rss_bytes': 100, 'pss_bytes': 60, 'fds': 3, 'threads': 2,
                 'cpu_seconds': 1.0}]
        artifact = self.build('attached', [identity(), census(1.0, 'periodic', rows)])
        self.assertEqual(artifact['trials'][0]['resource_series'][0]['pss_bytes'], 60)

    def test_cleanup_evidence_from_a_failed_trial_is_kept(self):
        artifact = self.build('attached', [
            identity(),
            {'event': 'trial_failure', 'error': 'boom'},
            {'event': 'trial_failure_process', 'exit_code': 101, 'killed_by_driver': False},
            {'event': 'trial_cleanup_failure', 'stream': 'stdin', 'error': 'EBADF'}])
        detail = artifact['trials'][0]['failure_detail']
        self.assertEqual([row['event'] for row in detail],
                         ['trial_failure_process', 'trial_cleanup_failure'])

    def test_the_outcome_each_case_exists_to_produce_is_kept_whole(self):
        """These were dropped, so Experiment 0006 cited data its artifact lacked."""
        artifact = self.build('dominant', [
            identity(),
            {'event': 'fairness', 'phase': 1, 'active_producers': 16, 'min_bytes': 4,
             'median_bytes': 4, 'max_bytes': 540, 'starved_producers': 0},
            {'event': 'reference_state', 'producer': 0, 'bytes_compared': 45939130,
             'equal': True, 'grid': '80x24'},
            {'event': 'overload_outcome', 'input_admission_rejections': 0,
             'output_backpressure_events': 5645046, 'observer_gap_bytes': 1265675432},
            {'event': 'observers_detached', 'released': 64, 'elapsed_seconds': 30.0}])
        trial = artifact['trials'][0]
        self.assertEqual(trial['fairness'][0]['max_bytes'], 540)
        self.assertEqual(trial['fairness'][0]['starved_producers'], 0)
        self.assertTrue(trial['reference_state']['equal'])
        self.assertEqual(trial['reference_state']['bytes_compared'], 45939130)
        self.assertEqual(trial['overload_outcome']['output_backpressure_events'], 5645046)
        self.assertEqual(trial['observers_detached']['released'], 64)

    def test_fairness_is_kept_per_phase_rather_than_last_one_wins(self):
        artifact = self.build('attached', [
            identity(),
            {'event': 'fairness', 'phase': 0, 'min_bytes': 1},
            {'event': 'fairness', 'phase': 1, 'min_bytes': 2}])
        self.assertEqual([row['phase'] for row in artifact['trials'][0]['fairness']], [0, 1])

    def test_a_summary_verdict_above_the_trial_is_recomputed_not_copied(self):
        """A correctly recorded failed trial under a clean summary verdict.

        Each trial's own pass bit was checked; everything above it was copied
        through, so `all_trials_passed: true` over a failing result archived
        without complaint.
        """
        root = Path(self.directory)
        write(root, 'trial.jsonl', [identity(), {'event': 'trial_failure', 'error': 'boom'}])
        (root / 'summary.json').write_text(json.dumps(
            {'results': [{'case': 'attached', 'trial': 1, 'passed': False,
                          'path': 'trial.jsonl'}],
             'all_trials_passed': True}))
        sys.argv = ['archive', '--input', str(root), '--output', str(root / 'a.json')]
        with self.assertRaises(SystemExit) as raised:
            archive.main()
        self.assertIn('all_trials_passed', str(raised.exception))

    def test_a_target_rollup_its_own_records_do_not_support_is_refused(self):
        root = Path(self.directory)
        write(root, 'trial.jsonl', [
            identity(configuration={'sessions': 1, 'active': 0, 'mode': 'idle', 'raw': True}),
            {'event': 'idle_cpu_target', 'passed': False, 'measurement_complete': True,
             'acceptance_duration': True},
            {'event': 'trial_result', 'passed': True}])
        (root / 'summary.json').write_text(json.dumps(
            {'results': [{'case': 'idle-64', 'trial': 1, 'passed': True, 'path': 'trial.jsonl',
                          'idle_cpu_target_applicable': True,
                          'idle_cpu_target_passed': True,
                          'idle_cpu_measurement_complete': True,
                          'idle_cpu_acceptance_duration': True,
                          'latency_targets_applicable': False,
                          'latency_targets_passed': None,
                          'latency_targets_missing': [],
                          'latency_measurements_complete': None}]}))
        sys.argv = ['archive', '--input', str(root), '--output', str(root / 'a.json')]
        with self.assertRaises(SystemExit) as raised:
            archive.main()
        self.assertIn('idle_cpu_target_passed', str(raised.exception))

    def test_a_claimed_full_matrix_must_actually_be_one(self):
        root = Path(self.directory)
        write(root, 'trial.jsonl', [identity(), {'event': 'trial_result', 'passed': True}])
        (root / 'summary.json').write_text(json.dumps(
            {'results': [{'case': 'attached', 'trial': 1, 'passed': True, 'path': 'trial.jsonl'}],
             'full_matrix_executed': True, 'repeats': 1, 'smoke': False}))
        sys.argv = ['archive', '--input', str(root), '--output', str(root / 'a.json')]
        with self.assertRaises(SystemExit) as raised:
            archive.main()
        self.assertIn('full_matrix_executed', str(raised.exception))

    def test_a_fixed_cohort_is_retained_so_turnover_cannot_hide_a_slope(self):
        """Totals over "whatever was measurable" are not comparable between samples.

        A costly process leaving while a cheap one arrives leaves the measured
        count flat and moves the totals anyway, so a count-only comparability
        check cannot tell the two apart.
        """
        events = [identity(),
                  census(0.0, 'measurement_start', [proc(1, 100, 5), proc(2, 900, 40)]),
                  census(5.0, 'periodic', [proc(1, 100, 5), proc(3, 900, 40)]),
                  census(10.0, 'measurement_end', [proc(1, 100, 5), proc(4, 900, 40)])]
        artifact = self.build('attached', events)
        trial = artifact['trials'][0]
        self.assertEqual(trial['resource_cohort']['cohort'], 1, 'only pid 1 survives every census')
        for row in trial['resource_series']:
            self.assertEqual(row['cohort_processes'], 1)
            self.assertEqual(row['cohort_fds'], 5, 'the churning pid is excluded')
            self.assertEqual(row['fds'], 45, 'the whole-tree total is kept alongside it')

    def test_a_run_without_a_measurement_window_says_so_rather_than_inventing_a_cohort(self):
        events = [identity(), census(0.0, 'periodic', [proc(1, 100, 5)])]
        artifact = self.build('attached', events)
        cohort = artifact['trials'][0]['resource_cohort']
        self.assertIsNone(cohort['cohort'])
        self.assertIn('measurement window', cohort['reason'])

    def test_a_summary_that_disagrees_with_the_trial_is_refused(self):
        """A file holding a failure was published as passing, from the summary."""
        with self.assertRaises(SystemExit):
            self.build('attached', [identity(), {'event': 'trial_failure', 'error': 'boom'}],
                       passed=True)

    def test_a_file_truncated_at_a_line_boundary_is_refused(self):
        """It parses cleanly and ends early, so the other two checks miss it."""
        with self.assertRaises(SystemExit):
            self.build('attached', [identity(), census(1.0, 'periodic', [])], terminal=False)

    def test_final_budget_values_survive_alongside_the_peak(self):
        """The peak proves the pressure; the last value proves it was given back."""
        events = [identity()] + [
            {'event': 'budget', 'phase': phase, 'name': 'views', 'used': used, 'limit': 64}
            for phase, used in (('under_load', 63), ('settled', 7), ('forgotten', 0))]
        peaks = self.build('attached', events)['trials'][0]['budget_peaks']['views']
        self.assertEqual(peaks['used_peak'], 63)
        self.assertEqual(peaks['final_used'], 0)
        self.assertEqual(peaks['final_phase'], 'forgotten')

    def test_checkpoint_records_are_retained(self):
        artifact = self.build('attached', [
            identity(),
            {'event': 'checkpoint', 'phase': 'baseline', 'requested_live_bytes': 10},
            {'event': 'checkpoint', 'phase': 'closed', 'requested_live_bytes': 10}])
        kept = artifact['trials'][0]['checkpoints']
        self.assertEqual([row['phase'] for row in kept], ['baseline', 'closed'])

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
