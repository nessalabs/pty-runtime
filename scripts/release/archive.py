#!/usr/bin/env python3
"""Turn a load run's raw output into the experiment artifact under docs/experiments.

ADR 0004 asks that integrated results record limits and workload, failures and
timeouts, latency distributions, and resource peaks and cleanup. Experiment 0005
was assembled by hand and kept only the final census and a filtered event list,
so the resource series behind six of its cases could not be audited afterwards.
Retention is defined here, in code, rather than by whoever ran the matrix.

What is kept, and why not simply everything: a 130-trial matrix emits roughly a
hundred thousand `budget` records and a per-process census row for every sampled
process, which is far larger than a reviewable artifact. So the series are
reduced along the axis that answers a question, and the reduction is named in
the artifact itself under `retention`.
"""
import argparse
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from load_support import matrix, reporting

# Keeping the full 1025-bucket histogram for every boundary of every trial is
# most of the artifact's size and almost all of it zeros. The occupied span
# preserves the distribution exactly.
def trim(buckets):
    occupied = [index for index, count in enumerate(buckets) if count]
    if not occupied:
        return {'first_bucket': None, 'counts': []}
    return {'first_bucket': occupied[0], 'counts': buckets[occupied[0]:occupied[-1] + 1]}


# Bumped whenever what is kept changes, and stamped into every artifact. An
# artifact built by an older policy is not wrong, but it holds less, and a
# reader comparing two of them needs to know which is which without guessing.
RETENTION_VERSION = 7

# Steady state, and the quiescence that has to follow it.
FULL_ROW_PHASES = ('measurement_end', 'closed')

# Records a trial emits exactly once, or not at all. Assigning any of these
# twice loses the first, which is how contradictory evidence hides inside a file
# that parses cleanly.
SINGLE_VALUED = ('identity', 'aggregate', 'throughput', 'fixture_rtt', 'cpu_interval',
                 'producer_start_skew', 'trial_result', 'trial_failure', 'reference_state',
                 'overload_outcome', 'observers_detached', 'stalled_sink', 'runtime_options',
                 'complete', 'start', 'idle_cpu_target')


def totals(sample):
    rows = sample.get('processes') or []
    def total(field):
        values = [row[field] for row in rows if row.get(field) is not None]
        return sum(values) if values else None
    return {'monotonic_seconds': sample.get('monotonic_seconds'),
            'phase': sample.get('phase'),
            'tree_processes': sample.get('tree_processes'),
            'measured_processes': len(rows),
            'rss_bytes': total('rss_bytes'),
            # Linux records proportional set size so shared pages are not
            # counted once per process. Dropping it here would lose the only
            # series that supports proportional-memory analysis, because the
            # per-process rows it comes from are not kept for every sample.
            'pss_bytes': total('pss_bytes'),
            'fds': total('fds'),
            'threads': total('threads'), 'cpu_seconds': total('cpu_seconds'),
            'zombies': len(sample.get('zombies') or []),
            'unavailable_pids': len(sample.get('unavailable_pids') or [])}


def measured_pids(sample):
    """The processes a census measured, named by incarnation where it can be.

    A pid is a number the kernel reuses. Keyed on the number alone, a recycled
    pid stays in the cohort and its new occupant's memory, descriptors and
    threads are fitted as though they were the original's - which is the same
    substitution the cohort exists to exclude, one level down. `start_ticks` is
    None on collectors that cannot name an incarnation, and the key degrades to
    the pid there rather than pretending otherwise.
    """
    return {(row['pid'], row.get('start_ticks'))
            for row in (sample.get('processes') or []) if row.get('pid') is not None}


def cohort_totals(samples, rows):
    """Add a second series taken over one fixed set of processes.

    Totals over "whatever was measurable this time" are not comparable between
    samples. A census that loses a costly process and gains a cheap one leaves
    the measured *count* perfectly flat while the totals move for a reason that
    has nothing to do with the resource — so turnover can mask a real slope, and
    a count-only comparability check cannot see it.

    The cohort is the set of processes present in *every* census of the
    measurement window, so its series is comparable by construction. It cannot
    see a leak that lives in the churn itself, which is why the whole-tree
    counts (`tree_processes`, `measured_processes`) are kept alongside it.
    """
    marks = {sample.get('phase'): sample.get('monotonic_seconds') for sample in samples}
    opened, closed = marks.get('measurement_start'), marks.get('measurement_end')
    if opened is None or closed is None:
        # Without both markers there is no window, and an intersection over a
        # trial's whole life is empty by construction: it ends with a tree of
        # one. Saying so is better than publishing a cohort of nothing.
        return {'cohort': None, 'reason': 'the run recorded no measurement window'}
    inside = [index for index, sample in enumerate(samples)
              if opened <= (sample.get('monotonic_seconds') or 0) <= closed]
    if not inside:
        return {'cohort': None, 'reason': 'no census fell inside the measurement window'}
    cohort = set.intersection(*(measured_pids(samples[index]) for index in inside))
    for index in inside:
        sample, row = samples[index], rows[index]
        members = [entry for entry in (sample.get('processes') or [])
                   if (entry.get('pid'), entry.get('start_ticks')) in cohort]
        def total(field):
            values = [entry[field] for entry in members if entry.get(field) is not None]
            return sum(values) if values else None
        row['cohort_processes'] = len(members)
        for field in ('rss_bytes', 'pss_bytes', 'fds', 'threads', 'cpu_seconds'):
            row['cohort_' + field] = total(field)
    return {'cohort': len(cohort), 'window_samples': len(inside),
            'reason': None if cohort else
                      'no process was measurable in every census of the window'}


def summarize(path, keep_process_rows):
    trial = {'file': path.name, 'identity': None, 'run': None, 'failure_detail': [],
             'checkpoints': [], 'fairness': [], 'reference_state': None,
             'overload_outcome': None, 'targets': [],
             'observers_detached': None, 'latency_targets': [],
             'latency': [], 'resource_series': [], 'budget_peaks': {},
             'aggregate': None, 'throughput': None, 'fixture_rtt': None,
             'cpu_interval': None, 'producer_start_skew': None,
             'trial_result': None, 'failure': None, 'stderr': [],
             'census_full': [], 'ledger_totals': None, 'resource_cohort': None,
             'terminal_records': 0, 'stalled_sink': None, 'runtime_options': None,
             'producers': [], 'operation_failures': [], 'record_counts': {}}
    gap = delivered = verified = 0
    censuses = []
    for line in path.read_text().splitlines():
        try:
            event = json.loads(line)
        except json.JSONDecodeError as error:
            # Skipping a line here would quietly shrink the evidence while the
            # artifact still reported the run as complete.
            raise ValueError(f'{path.name}: unreadable line: {error}') from error
        kind = event.get('event')
        # Every field below that holds one record per trial was assigned rather
        # than accumulated, so a second record of that kind silently replaced
        # the first. That was reported three times - terminal records, latency
        # targets, and now identity - each time as one field. It is one bug, so
        # count them all and refuse any that appears twice, rather than waiting
        # to be told about the next field.
        if kind in SINGLE_VALUED:
            trial['record_counts'][kind] = trial['record_counts'].get(kind, 0) + 1
        if kind == 'identity':
            # Split once here: the shared half is hoisted to the artifact and the
            # varying half stays per trial. Repeating a 311-file inventory and
            # the toolchain output for every trial was three quarters of the
            # artifact's size and told a reader nothing new after the first.
            varying = ('configuration', 'command', 'repeat', 'owner_pid',
                       'binary_sha256', 'sample_seconds', 'smoke')
            trial['run'] = {key: event[key] for key in varying if key in event}
            trial['identity'] = {key: value for key, value in event.items()
                                 if key not in varying}
        elif kind in ('latency_target', 'idle_cpu_target'):
            # The idle cases are judged by idle_cpu_target exactly as the others
            # are by latency_target. Keeping only the summary boolean meant the
            # 500-session pass could not be checked without reconstructing it
            # from cpu_interval and the driver's source.
            trial['targets' if kind == 'idle_cpu_target' else 'latency_targets'].append(event)
        elif kind == 'latency':
            row = {key: event[key] for key in
                   ('boundary', 'successes', 'failures', 'unavailable',
                    'p50_us', 'p95_us', 'p99_us', 'max_us', 'bucket_width_us')}
            buckets = event.get('buckets') or []
            row['bucket_count'] = len(buckets)
            row['histogram_ceiling_us'] = (len(buckets) - 1) * event['bucket_width_us'] if buckets else None
            row['overflow_bucket_samples'] = buckets[-1] if buckets else None
            row['distribution'] = trim(buckets)
            trial['latency'].append(row)
        elif kind == 'physical_resources':
            censuses.append(event)
            trial['resource_series'].append(totals(event))
            # Per-process rows answer two questions and no others: what the tree
            # looks like in steady state, and whether it emptied afterwards. At
            # 128 sessions a census carries ~385 rows, so keeping every periodic
            # sample was 90% of the artifact and nineteen near-identical copies
            # of the same answer. Accumulation is already in `resource_series`.
            if event.get('phase') in FULL_ROW_PHASES and (
                    keep_process_rows or event.get('phase') == 'closed'):
                trial['census_full'].append(event)
        elif kind == 'budget':
            key = event['name']
            peak = trial['budget_peaks'].setdefault(
                key, {'used_peak': 0, 'limit': event['limit'], 'samples': 0,
                      'final_used': None, 'final_phase': None})
            peak['used_peak'] = max(peak['used_peak'], event['used'])
            peak['samples'] += 1
            # The peak proves what the pressure reached; the last value proves
            # the logical resources were given back. The fixture asserts every
            # budget is zero at `forgotten`, and keeping only peaks discarded
            # that proof while keeping the pressure it is paired with.
            peak['final_used'] = event['used']
            peak['final_phase'] = event.get('phase')
        elif kind == 'ledger':
            gap += event.get('gap_bytes', 0)
            delivered += event.get('total_bytes', 0)
            verified += event.get('verified_bytes', 0)
        elif kind in ('aggregate', 'throughput', 'fixture_rtt', 'cpu_interval',
                      'producer_start_skew', 'trial_result',
                      # The outcomes each case exists to produce. Omitting these
                      # made the artifact silently unable to support the very
                      # claims the experiment cites it for - the same failure
                      # this script was written to stop.
                      'reference_state', 'overload_outcome', 'observers_detached',
                      # `stalled-sink` is the only case that proves a slow
                      # publisher is held to one in-flight publication with a
                      # bounded payload, and its whole proof is in this one
                      # record. Omitting it left those trials archived with
                      # nothing a reader could audit the case against.
                      'stalled_sink',
                      # One per trial: the runtime configuration the numbers
                      # were produced under.
                      'runtime_options'):
            if kind == 'trial_result':
                trial['terminal_records'] += 1
            trial[kind] = event
        elif kind in ('producer_done', 'producer_end', 'producer_writes'):
            # ADR 0004 asks for per-PTY progress and blocking, and LOAD.md says
            # these are retained. They were not: aggregate throughput and the
            # fairness extrema cannot reconstruct which PTY blocked, for how
            # long, across how many write calls, or how often a write went
            # short. One row per producer per phase is the report itself, so it
            # is kept rather than summarised.
            trial['producers'].append(event)
        elif kind == 'operation_failure':
            # Rare by construction and worthless in aggregate: a resize or
            # cancel that failed is exactly the record a reader needs whole.
            trial['operation_failures'].append(event)
        elif kind == 'checkpoint':
            # Requested live and peak Rust allocations, allocation counts, live
            # readers and reader scratch, at baseline, the measurement
            # boundaries and shutdown. Without these the artifact cannot show
            # allocation peaks or a return to baseline.
            trial.setdefault('checkpoints', []).append(event)
        elif kind == 'fairness':
            # One per phase, so a list rather than a single value.
            trial.setdefault('fairness', []).append(event)
        elif kind == 'trial_failure':
            # Counted as well as kept. Assigning here overwrote a previous
            # terminal record, so a file holding `passed: false` followed by
            # `passed: true` published the second and hid the first.
            trial['terminal_records'] += 1
            trial['failure'] = event
        elif kind in ('trial_failure_process', 'trial_cleanup_failure'):
            # A trial that never reaches its closing census has no other record
            # of how its child ended or whether its pipes closed. That is
            # precisely the trial whose cleanup a reader most needs.
            trial.setdefault('failure_detail', []).append(event)
        elif kind == 'stderr':
            trial['stderr'].append(event.get('text'))
    if delivered:
        trial['ledger_totals'] = {'gap_bytes': gap, 'total_bytes': delivered,
                                  'verified_bytes': verified}
    trial['resource_cohort'] = cohort_totals(censuses, trial['resource_series'])
    return trial


def recomputed_summary(summary, trials):
    """The summary as the retained trial records themselves support it.

    Checking each trial's own pass bit was not enough. Everything above a trial
    - `all_trials_passed`, `full_matrix_executed`, and the target rollup - was
    copied through unexamined, so a summary claiming a clean full matrix over
    results that say otherwise archived without complaint. The summary is an
    index of the trials; an index that disagrees with them is exactly the thing
    an archiver must refuse to publish.
    """
    rows, recomputed = [], []
    for row, trial in zip(summary['results'], trials):
        configuration = (trial['run'] or {}).get('configuration')
        if configuration is None:
            raise SystemExit(
                f'{row["path"]} records no configuration, so the target verdicts '
                f'summary.json publishes for it cannot be checked against anything')
        # Building the map silently kept the last record per boundary while the
        # artifact retained them all, so a boundary recorded `passed: false` and
        # then `passed: true` recomputed as passing against evidence that holds
        # a failure. A boundary is judged once per trial, as is idle CPU.
        boundaries = [event['boundary'] for event in trial['latency_targets']]
        repeated = sorted({name for name in boundaries if boundaries.count(name) > 1})
        if repeated:
            raise SystemExit(
                f'{row["path"]} records more than one latency target for {repeated}; '
                f'a boundary is judged once, and which record describes it cannot be '
                f'decided here')
        if len(trial['targets']) > 1:
            raise SystemExit(
                f'{row["path"]} records {len(trial["targets"])} idle CPU targets; '
                f'a trial is judged once')
        latency = {event['boundary']: event for event in trial['latency_targets']}
        idle = trial['targets'][-1] if trial['targets'] else {}
        targets = reporting.targets_from_events(latency, idle, configuration)
        recomputed.append(targets)
        rows.append({**row, **targets})
    return rows, recomputed, {'all_trials_passed': all(row['passed'] for row in rows),
                              'target_rollup': reporting.rollup(rows)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', type=Path, required=True, help='a load.py --output directory')
    parser.add_argument('--output', type=Path, required=True, help='artifact JSON to write')
    parser.add_argument('--full-process-rows', action='append', default=['resources-'],
                        help='case name prefixes whose per-process census rows are kept whole')
    args = parser.parse_args()
    summary = json.loads((args.input / 'summary.json').read_text())
    # An archiver that publishes nothing as though it were something is the
    # failure this script exists to prevent, and `all([])` is True: a summary
    # with no results archived at exit 0 carrying zero trials and
    # `all_trials_passed: true`.
    if not summary.get('results'):
        raise SystemExit(
            'summary.json lists no trials; an artifact holding no evidence would '
            'publish "every trial passed" over nothing')
    # One file per trial. Two rows pointing at the same output would archive as
    # two trials holding one trial's evidence, which inflates any count taken
    # from the summary - including the full-matrix claim below.
    paths = [row['path'] for row in summary['results']]
    if len(set(paths)) != len(paths):
        duplicated = sorted({path for path in paths if paths.count(path) > 1})
        raise SystemExit(
            f'summary.json lists the same trial output more than once: {duplicated}; '
            f'the artifact would hold one trial\'s evidence under several trials')
    trials = []
    for row in summary['results']:
        path = args.input / row['path']
        if not path.exists():
            # The summary still lists it, so skipping would produce an artifact
            # that looks complete while holding fewer trials than the run it
            # claims to describe.
            raise SystemExit(f'{row["path"]} is named by summary.json but missing; '
                             f'the run is incomplete and archiving it would hide that')
        keep = any(row['case'].startswith(prefix) for prefix in args.full_process_rows)
        trial = summarize(path, keep)
        if trial['identity'] is None:
            raise SystemExit(f'{row["path"]} carries no identity record; '
                             f'its measurements cannot be attributed to a revision')
        repeated = {kind: count for kind, count in trial['record_counts'].items() if count > 1}
        if repeated:
            raise SystemExit(
                f'{row["path"]} records {repeated} more than once; each of these '
                f'describes the whole trial, so a second one contradicts the first '
                f'and which is authoritative cannot be decided here')
        # The summary's coordinate against the file's own. A run whose files each
        # say `repeat 1` while the summary labels them 1 to 5 is one trial copied
        # five times, and every count taken from the summary - including the
        # full-matrix claim - is then counting labels rather than trials.
        embedded = trial['run'].get('repeat')
        if embedded is not None and embedded != row['trial']:
            raise SystemExit(
                f'{row["path"]} identifies itself as repeat {embedded} but summary.json '
                f'lists it as trial {row["trial"]}; the coordinate and the evidence '
                f'disagree')
        # A file truncated at a line boundary parses cleanly and ends early, so
        # neither the missing-file nor the malformed-line check sees it. Without
        # this the trial would inherit `passed` from the summary while holding
        # no result at all.
        if trial['trial_result'] is None and trial['failure'] is None:
            raise SystemExit(
                f'{row["path"]} ends without a trial_result or a trial_failure; '
                f'summary.json records passed={row["passed"]} for it, which nothing '
                f'in the file supports')
        # Presence was not enough: a file holding a trial_failure was still
        # published as passing, because the pass bit came from the summary and
        # nothing compared the two. The file is the record; the summary is an
        # index of it, and an index that disagrees is the thing to catch.
        recorded = trial['trial_result']['passed'] if trial['trial_result'] else False
        # Counted rather than merely typed. Refusing only the *combination* of a
        # result and a failure left two records of the same kind overwriting
        # each other, so `passed: false` followed by `passed: true` published the
        # second and the artifact hid its own contradictory evidence. A trial
        # ends once.
        if trial['terminal_records'] != 1:
            raise SystemExit(
                f'{row["path"]} carries {trial["terminal_records"]} terminal records; '
                f'a trial ends exactly once, and which of several describes it '
                f'cannot be decided here')
        if recorded != row['passed']:
            raise SystemExit(
                f'{row["path"]} records passed={recorded} but summary.json says '
                f'passed={row["passed"]}; the artifact would publish a verdict its '
                f'own trial data contradicts')
        trial['case'] = row['case']
        trial['trial'] = row['trial']
        trial['passed'] = row['passed']
        trials.append(trial)
    # Everything above the individual trial, checked against the trials rather
    # than republished. A summary carrying a correctly recorded failed trial
    # alongside `all_trials_passed: true`, or a target rollup that counts a
    # verdict its own target records do not support, archived cleanly before
    # this.
    _, recomputed_targets, recomputed = recomputed_summary(summary, trials)
    for row, expected in zip(summary['results'], recomputed_targets):
        disagreement = {key: (row.get(key), value) for key, value in expected.items()
                        if key in row and row[key] != value}
        if disagreement:
            raise SystemExit(
                f'{row["path"]}: summary.json records target verdicts its own retained '
                f'target records do not support: {disagreement}')
    for key, value in recomputed.items():
        # Only what the summary actually publishes is checked. A summary that
        # never made the claim is not overclaiming.
        if key in summary and summary[key] != value:
            raise SystemExit(
                f'summary.json records {key}={summary.get(key)!r} while the trials it '
                f'indexes give {value!r}; the artifact would publish a verdict its own '
                f'evidence contradicts')
    if summary.get('full_matrix_executed'):
        # Only a `true` is checked. A run that says it was not the full matrix
        # is not overclaiming, and refusing it would make an honest partial run
        # unarchivable.
        #
        # Case labels and the scalar `repeats` are not enough on their own: a
        # summary naming every default case once, with `repeats: 5`, satisfied
        # both and described 28 trials rather than 140. What the claim means is
        # that each case was actually run `repeats` times, so count the trials.
        repeats = summary.get('repeats', 0)
        executed = {}
        for trial in trials:
            executed.setdefault(trial['case'], set()).add(trial['trial'])
        defined = matrix.cases(summary.get('smoke', False))
        expected = set(matrix.default_selection(summary.get('smoke', False)))
        short = {case: len(executed.get(case, ())) for case in sorted(expected)
                 if len(executed.get(case, ())) < repeats}
        if summary.get('smoke') or repeats < 5 or short:
            raise SystemExit(
                f'summary.json claims full_matrix_executed while smoke={summary.get("smoke")}, '
                f'repeats={repeats}, and these default cases carry fewer distinct trials '
                f'than that: {short or "none"}')
        # Counting the trials was still not the claim. `--seconds 1` runs every
        # default case five times and is not the matrix: LOAD.md defines it as
        # 60-second post-warmup trials, and the driver's own help says a
        # different duration answers a different question. The workload each
        # trial actually ran is retained, so compare it.
        altered = {}
        for trial in trials:
            if trial['case'] not in expected:
                continue
            ran = trial['run']['configuration']
            changed = {key: (value, ran.get(key)) for key, value in defined[trial['case']].items()
                       if ran.get(key) != value}
            if changed:
                altered[f'{trial["case"]}-{trial["trial"]}'] = changed
        if altered:
            raise SystemExit(
                f'summary.json claims full_matrix_executed, but these trials did not run '
                f'the matrix workload: {altered}')
    shared = [trial.pop('identity') for trial in trials]
    common = shared[0] if shared else {}
    # Carry the trial's coordinates with it: a bare list of differing identities
    # says a divergence happened without saying which measurements it affected,
    # which is the only thing a reader needs from it.
    divergent = [{'case': trial['case'], 'trial': trial['trial'], 'file': trial['file'],
                  'identity': identity}
                 for trial, identity in zip(trials, shared) if identity != common]
    artifact = {
        'retention_version': RETENTION_VERSION,
        'summary': summary,
        'identity': common,
        'identity_divergent_trials': divergent,
        'retention': {
            'latency_distribution': 'full histogram trimmed to its occupied bucket span; '
                                    'bucket width, count, ceiling and overflow count retained',
            'resource_series': 'every periodic census reduced to tree totals over time, '
                               'which is what shows accumulation, plus totals over the '
                               'cohort of processes measurable in every census of the '
                               'measurement window - a fixed population, so its series is '
                               'comparable between samples where the whole-tree totals are '
                               'not; the cohort size is under each trial as resource_cohort',
            'summary_verdicts': 'recomputed from the retained target records and refused if '
                                'they disagree; all_trials_passed, the target rollup and a '
                                'claimed full_matrix_executed are checked, not copied, and the '
                                'full-matrix claim is counted in trials rather than case labels',
            'terminal_records': 'the number of trial_result or trial_failure records the file '
                                'held; anything but exactly one is refused, so a later record '
                                'cannot overwrite an earlier contradictory one',
            'record_counts': 'how many of each once-per-trial record the file held. Any of them '
                             'appearing twice is refused rather than assigned over, and the '
                             'counts are kept so a reader can see the check had something to '
                             'check. The summary coordinate is also matched against the trial\'s '
                             'own recorded repeat',
            'process_identity': 'census rows carry start_ticks and ppid where the collector can '
                                'read them, and the cohort is keyed on (pid, start_ticks): a '
                                'recycled pid is a different process and leaves the cohort',
            'process_rows': 'kept whole at measurement_end and at the closing census - steady '
                            'state and the quiescence that must follow it - and only for cases '
                            'named in full_process_rows, except the closing census which is '
                            'kept for every case because it is the cleanup proof',
            'budget_series': 'per-name peak used against its limit, with the sample count; '
                             'individual budget records are not retained',
            'full_process_row_cases': args.full_process_rows,
            'identity': 'hoisted to the artifact once; fields that vary per trial are '
                        'under each trial as `run`, and any trial whose shared half differed '
                        'is listed in `identity_divergent_trials` rather than silently merged',
            'case_outcomes': 'fairness, reference_state, overload_outcome, stalled_sink and '
                             'observers_detached are kept whole: they are what their '
                             'cases exist to produce, and a summary of them is not evidence',
            'per_producer': 'producer_done, producer_end and producer_writes are kept whole, '
                            'one row per producer per phase - bytes, blocked-write duration, '
                            'maximum backpressure wait, write calls, EAGAIN and partial-write '
                            'counts. This is ADR 0004\'s per-PTY progress and blocking report, '
                            'which nothing else in the artifact can reconstruct',
            'operation_failures': 'kept whole; rare by construction and meaningless in aggregate',
            'not_retained': 'per-producer ledger rows (totalled), producer_start records, '
                            'individual budget records, and per-process rows for non-resource '
                            'cases outside the closing census',
        },
        'trials': trials,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(artifact, sort_keys=True) + '\n')
    print(f'{args.output} {args.output.stat().st_size} bytes, {len(trials)} trials')


if __name__ == '__main__':
    main()
