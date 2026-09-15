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


def identity(case='attached', **extra):
    # A complete record: the archiver requires the fields that make a trial
    # attributable, because an identity-shaped event is not attribution - and
    # the configuration a case's label implies, because it checks that too.
    return {'event': 'identity', 'platform': 'Linux', 'machine': 'x86_64',
            'source_head': 'abc', 'diff_sha256': 'd', 'binary_sha256': 'b',
            'descriptor_limit': {'soft': 65535, 'hard': 65535, 'unlimited': False},
            'source_inventory': [{'path': f'f{n}', 'sha256': 'x'} for n in range(300)],
            'configuration': archive.matrix.cases()[case],
            'command': ['release_load'], 'repeat': 1, **extra}


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
        # The archiver checks the case label against the workload the trial
        # retained, so a fixture has to carry the configuration its label
        # implies. Tests about retention should not each restate it.
        events = [{**event, 'configuration': archive.matrix.cases()[case]}
                  if event.get('event') == 'identity' else event
                  for event in events]
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
        self.assertEqual(artifact['trials'][0]['run']['configuration'],
                         archive.matrix.cases()['attached'])

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
        write(root, 'b.jsonl', [identity(source_head='different', repeat=2), done])
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
            identity('idle-64'),
            # 2 % against a 1 % ceiling: the record's own numbers give False, so
            # the summary's True is the disagreement under test.
            {'event': 'idle_cpu_target', 'passed': False, 'measurement_complete': True,
             'measured_core_percent': 2.0, 'target_core_percent': 1.0,
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

    def test_two_terminal_records_of_the_same_kind_are_refused(self):
        """A trial ends once.

        Refusing only the *combination* of a result and a failure left two
        records of the same kind overwriting each other, so `passed: false`
        followed by `passed: true` published the second and hid the first.
        """
        root = Path(self.directory)
        write(root, 'trial.jsonl', [identity(),
                                    {'event': 'trial_result', 'passed': False},
                                    {'event': 'trial_result', 'passed': True}])
        (root / 'summary.json').write_text(json.dumps(
            {'results': [{'case': 'attached', 'trial': 1, 'passed': True,
                          'path': 'trial.jsonl'}]}))
        sys.argv = ['archive', '--input', str(root), '--output', str(root / 'a.json')]
        with self.assertRaises(SystemExit) as raised:
            archive.main()
        self.assertIn("{'trial_result': 2}", str(raised.exception))

    def test_one_trial_output_cannot_stand_in_for_several_trials(self):
        root = Path(self.directory)
        write(root, 'trial.jsonl', [identity(), {'event': 'trial_result', 'passed': True}])
        (root / 'summary.json').write_text(json.dumps(
            {'results': [{'case': 'attached', 'trial': n, 'passed': True, 'path': 'trial.jsonl'}
                         for n in (1, 2)]}))
        sys.argv = ['archive', '--input', str(root), '--output', str(root / 'a.json')]
        with self.assertRaises(SystemExit) as raised:
            archive.main()
        self.assertIn('more than once', str(raised.exception))

    def test_a_full_matrix_claim_counts_trials_not_case_labels(self):
        """Every default case named once, `repeats: 5`, and 28 trials not 140."""
        root = Path(self.directory)
        results = []
        for index, case in enumerate(archive.matrix.default_selection()):
            name = f'{case}-1.jsonl'
            write(root, name, [identity(case), {'event': 'trial_result', 'passed': True}])
            results.append({'case': case, 'trial': 1, 'passed': True, 'path': name})
        (root / 'summary.json').write_text(json.dumps(
            {'results': results, 'full_matrix_executed': True, 'repeats': 5, 'smoke': False}))
        sys.argv = ['archive', '--input', str(root), '--output', str(root / 'a.json')]
        with self.assertRaises(SystemExit) as raised:
            archive.main()
        self.assertIn('fewer distinct trials', str(raised.exception))

    def test_two_verdicts_for_one_boundary_are_refused(self):
        """Building the map kept the last record while the artifact kept both."""
        root = Path(self.directory)
        write(root, 'trial.jsonl', [
            identity(),
            {'event': 'latency_target', 'boundary': 'InputDispatch', 'passed': False,
             'measurement_complete': True},
            {'event': 'latency_target', 'boundary': 'InputDispatch', 'passed': True,
             'measurement_complete': True},
            {'event': 'trial_result', 'passed': True}])
        (root / 'summary.json').write_text(json.dumps(
            {'results': [{'case': 'attached', 'trial': 1, 'passed': True,
                          'path': 'trial.jsonl'}]}))
        sys.argv = ['archive', '--input', str(root), '--output', str(root / 'a.json')]
        with self.assertRaises(SystemExit) as raised:
            archive.main()
        self.assertIn('InputDispatch', str(raised.exception))

    def test_a_full_matrix_claim_requires_the_matrix_workload(self):
        """`--seconds 1` runs every case five times and is not the matrix."""
        root = Path(self.directory)
        results = []
        for case, config in archive.matrix.cases().items():
            if case in archive.matrix.HOST_DEPENDENT:
                continue
            for trial in range(1, 6):
                name = f'{case}-{trial}.jsonl'
                # `seconds` is an allowed override, so the case label stands;
                # what it is not is the matrix.
                write(root, name, [identity(case, configuration={**config, 'seconds': 1},
                                            repeat=trial),
                                   {'event': 'trial_result', 'passed': True}])
                results.append({'case': case, 'trial': trial, 'passed': True, 'path': name})
        (root / 'summary.json').write_text(json.dumps(
            {'results': results, 'full_matrix_executed': True, 'repeats': 5, 'smoke': False}))
        sys.argv = ['archive', '--input', str(root), '--output', str(root / 'a.json')]
        with self.assertRaises(SystemExit) as raised:
            archive.main()
        self.assertIn('did not run the matrix workload', str(raised.exception))

    def test_the_per_pty_progress_and_blocking_report_is_retained(self):
        """ADR 0004 asks for it per PTY, and nothing else can reconstruct it."""
        events = [identity(),
                  {'event': 'producer_done', 'producer': 0, 'phase': 1,
                   'write_blocked_ns': 42, 'max_backpressure_wait_ns': 7, 'write_calls': 9},
                  {'event': 'producer_writes', 'producer': 0, 'phase': 1,
                   'eagain_count': 3, 'partial_write_count': 1},
                  {'event': 'producer_end', 'producer': 0, 'phase': 1, 'elapsed_ns': 5},
                  {'event': 'stalled_sink', 'calls': 1, 'inflight': 1,
                   'max_payload_bytes': 4096},
                  {'event': 'runtime_options', 'value': 'opts'},
                  {'event': 'operation_failure', 'operation': 'resize_projected'}]
        artifact = self.build('stalled-sink', events)
        trial = artifact['trials'][0]
        kinds = [row['event'] for row in trial['producers']]
        self.assertEqual(sorted(kinds),
                         ['producer_done', 'producer_end', 'producer_writes'])
        self.assertEqual(trial['producers'][0]['write_blocked_ns'], 42)
        self.assertEqual(trial['stalled_sink']['max_payload_bytes'], 4096)
        self.assertEqual(trial['runtime_options']['value'], 'opts')
        self.assertEqual(len(trial['operation_failures']), 1)

    def test_every_once_per_trial_record_is_refused_twice_not_only_the_named_ones(self):
        """One bug reported three times as three fields, so it is checked as one."""
        for kind, extra in [('identity', {}),
                            ('throughput', {'accepted_bytes': 1}),
                            ('fixture_rtt', {'p99_us': 1}),
                            ('reference_state', {'equal': True}),
                            ('overload_outcome', {'observer_gaps': 0}),
                            ('runtime_options', {'value': 'a'}),
                            ('stalled_sink', {'calls': 1})]:
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                twice = [{'event': kind, **extra}, {'event': kind, **extra}]
                events = ([identity()] if kind != 'identity' else []) + twice
                write(root, 'trial.jsonl', events + [{'event': 'trial_result', 'passed': True}])
                (root / 'summary.json').write_text(json.dumps(
                    {'results': [{'case': 'attached', 'trial': 1, 'passed': True,
                                  'path': 'trial.jsonl'}]}))
                sys.argv = ['archive', '--input', str(root), '--output', str(root / 'a.json')]
                with self.assertRaises(SystemExit, msg=kind) as raised:
                    archive.main()
                self.assertIn(kind, str(raised.exception))

    def test_a_summary_coordinate_must_match_the_trial_it_points_at(self):
        """Five files each saying `repeat 1` is one trial copied five times."""
        root = Path(self.directory)
        write(root, 'a.jsonl', [identity(repeat=1), {'event': 'trial_result', 'passed': True}])
        write(root, 'b.jsonl', [identity(repeat=1), {'event': 'trial_result', 'passed': True}])
        (root / 'summary.json').write_text(json.dumps({'results': [
            {'case': 'attached', 'trial': 1, 'passed': True, 'path': 'a.jsonl'},
            {'case': 'attached', 'trial': 2, 'passed': True, 'path': 'b.jsonl'}]}))
        sys.argv = ['archive', '--input', str(root), '--output', str(root / 'a.json')]
        with self.assertRaises(SystemExit) as raised:
            archive.main()
        self.assertIn('identifies itself as repeat 1', str(raised.exception))

    def test_a_run_with_no_trials_is_not_a_passing_run(self):
        """`all([])` is True, so an empty summary published a passing artifact."""
        root = Path(self.directory)
        (root / 'summary.json').write_text(json.dumps(
            {'results': [], 'all_trials_passed': True}))
        sys.argv = ['archive', '--input', str(root), '--output', str(root / 'a.json')]
        with self.assertRaises(SystemExit) as raised:
            archive.main()
        self.assertIn('no trials', str(raised.exception))

    def test_a_recycled_pid_leaves_the_cohort(self):
        """A pid is a number the kernel reuses; the cohort is a set of processes."""
        def row(pid, ticks, fds):
            return {'pid': pid, 'start_ticks': ticks, 'rss_bytes': 10, 'fds': fds,
                    'threads': 1, 'cpu_seconds': 1.0, 'pss_bytes': None}
        events = [identity(),
                  census(0.0, 'measurement_start', [row(1, 100, 5), row(2, 200, 40)]),
                  # Same number, different process.
                  census(5.0, 'periodic', [row(1, 100, 5), row(2, 999, 40)]),
                  census(10.0, 'measurement_end', [row(1, 100, 5), row(2, 999, 40)])]
        artifact = self.build('attached', events)
        trial = artifact['trials'][0]
        self.assertEqual(trial['resource_cohort']['cohort'], 1,
                         'pid 2 is two different processes and belongs to neither cohort')
        for entry in trial['resource_series']:
            self.assertEqual(entry['cohort_fds'], 5)

    def test_a_verdict_its_own_measurement_contradicts_is_refused(self):
        """224 ms against a 20 ms ceiling, recorded as passing."""
        root = Path(self.directory)
        write(root, 'trial.jsonl', [
            identity(),
            {'event': 'latency_target', 'boundary': 'ProjectedOutput', 'passed': True,
             'measurement_complete': True, 'observed_p99_us': 224000,
             'target_p99_us': 20000, 'failures': 0, 'unavailable': 0},
            {'event': 'trial_result', 'passed': True}])
        (root / 'summary.json').write_text(json.dumps(
            {'results': [{'case': 'attached', 'trial': 1, 'passed': True,
                          'path': 'trial.jsonl'}]}))
        sys.argv = ['archive', '--input', str(root), '--output', str(root / 'a.json')]
        with self.assertRaises(SystemExit) as raised:
            archive.main()
        self.assertIn('measurements contradict', str(raised.exception))
        self.assertIn('224000', str(raised.exception))

    def test_a_target_judged_against_a_p99_the_run_did_not_measure_is_refused(self):
        """The verdict's numbers must be the run's numbers, not a second set."""
        root = Path(self.directory)
        write(root, 'trial.jsonl', [
            identity(),
            {'event': 'latency', 'boundary': 'ProjectedOutput', 'successes': 10,
             'failures': 0, 'unavailable': 0, 'p50_us': 100, 'p95_us': 200,
             'p99_us': 224000, 'max_us': 300000, 'bucket_width_us': 100, 'buckets': [1]},
            {'event': 'latency_target', 'boundary': 'ProjectedOutput', 'passed': True,
             'measurement_complete': True, 'observed_p99_us': 900,
             'target_p99_us': 20000, 'failures': 0, 'unavailable': 0},
            {'event': 'trial_result', 'passed': True}])
        (root / 'summary.json').write_text(json.dumps(
            {'results': [{'case': 'attached', 'trial': 1, 'passed': True,
                          'path': 'trial.jsonl'}]}))
        sys.argv = ['archive', '--input', str(root), '--output', str(root / 'a.json')]
        with self.assertRaises(SystemExit) as raised:
            archive.main()
        self.assertIn('its own latency records do not report', str(raised.exception))

    def test_an_identity_missing_its_mandatory_fields_is_refused(self):
        """An identity-shaped event is not attribution.

        Dropping `repeat` also bypassed the coordinate check entirely, by
        leaving it nothing to compare.
        """
        for field in ('repeat', 'source_head', 'binary_sha256', 'machine',
                      'descriptor_limit', 'configuration'):
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                record = identity()
                del record[field]
                write(root, 'trial.jsonl', [record, {'event': 'trial_result', 'passed': True}])
                (root / 'summary.json').write_text(json.dumps(
                    {'results': [{'case': 'attached', 'trial': 1, 'passed': True,
                                  'path': 'trial.jsonl'}]}))
                sys.argv = ['archive', '--input', str(root), '--output', str(root / 'a.json')]
                with self.assertRaises(SystemExit, msg=field) as raised:
                    archive.main()
                self.assertIn(field, str(raised.exception))

    def test_a_target_with_no_retained_measurement_is_refused(self):
        """Every target passing over an artifact holding no distribution at all."""
        root = Path(self.directory)
        write(root, 'trial.jsonl', [
            identity(),
            {'event': 'latency_target', 'boundary': 'ProjectedOutput', 'passed': True,
             'measurement_complete': True, 'observed_p99_us': 900,
             'target_p99_us': 20000, 'failures': 0, 'unavailable': 0},
            {'event': 'trial_result', 'passed': True}])
        (root / 'summary.json').write_text(json.dumps(
            {'results': [{'case': 'attached', 'trial': 1, 'passed': True,
                          'path': 'trial.jsonl'}]}))
        sys.argv = ['archive', '--input', str(root), '--output', str(root / 'a.json')]
        with self.assertRaises(SystemExit) as raised:
            archive.main()
        self.assertIn('retaining no latency measurement', str(raised.exception))

    def test_two_measurements_for_one_boundary_are_refused(self):
        root = Path(self.directory)
        def measured(p99):
            return {'event': 'latency', 'boundary': 'ProjectedOutput', 'successes': 10,
                    'failures': 0, 'unavailable': 0, 'p50_us': 100, 'p95_us': 200,
                    'p99_us': p99, 'max_us': p99, 'bucket_width_us': 100, 'buckets': [1]}
        write(root, 'trial.jsonl', [identity(), measured(900), measured(224000),
                                    {'event': 'trial_result', 'passed': True}])
        (root / 'summary.json').write_text(json.dumps(
            {'results': [{'case': 'attached', 'trial': 1, 'passed': True,
                          'path': 'trial.jsonl'}]}))
        sys.argv = ['archive', '--input', str(root), '--output', str(root / 'a.json')]
        with self.assertRaises(SystemExit) as raised:
            archive.main()
        self.assertIn('measured once per trial', str(raised.exception))

    def test_a_case_label_must_match_the_workload_the_trial_ran(self):
        """An idle raw trial archived cleanly as `reference`."""
        root = Path(self.directory)
        write(root, 'trial.jsonl', [identity('idle-64'),
                                    {'event': 'trial_result', 'passed': True}])
        (root / 'summary.json').write_text(json.dumps(
            {'results': [{'case': 'reference', 'trial': 1, 'passed': True,
                          'path': 'trial.jsonl'}]}))
        sys.argv = ['archive', '--input', str(root), '--output', str(root / 'a.json')]
        with self.assertRaises(SystemExit) as raised:
            archive.main()
        self.assertIn('ran a different workload', str(raised.exception))

    def test_the_documented_overrides_keep_the_case_label(self):
        """`--seconds` and `--staging-slots` are the only supported overrides."""
        config = {**archive.matrix.cases()['attached'], 'seconds': 3300,
                  'staging_slots': 16}
        root = Path(self.directory)
        write(root, 'trial.jsonl', [identity(configuration=config),
                                    {'event': 'trial_result', 'passed': True}])
        (root / 'summary.json').write_text(json.dumps(
            {'results': [{'case': 'attached', 'trial': 1, 'passed': True,
                          'path': 'trial.jsonl'}]}))
        out = root / 'a.json'
        sys.argv = ['archive', '--input', str(root), '--output', str(out)]
        archive.main()
        self.assertEqual(json.loads(out.read_text())['trials'][0]['case'], 'attached')

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
