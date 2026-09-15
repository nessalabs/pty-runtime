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
RETENTION_VERSION = 2

# Steady state, and the quiescence that has to follow it.
FULL_ROW_PHASES = ('measurement_end', 'closed')


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


def summarize(path, keep_process_rows):
    trial = {'file': path.name, 'identity': None, 'run': None, 'failure_detail': [],
             'checkpoints': [], 'fairness': [], 'reference_state': None,
             'overload_outcome': None,
             'observers_detached': None, 'latency_targets': [],
             'latency': [], 'resource_series': [], 'budget_peaks': {},
             'aggregate': None, 'throughput': None, 'fixture_rtt': None,
             'cpu_interval': None, 'producer_start_skew': None,
             'trial_result': None, 'failure': None, 'stderr': [],
             'census_full': [], 'ledger_totals': None}
    gap = delivered = verified = 0
    for line in path.read_text().splitlines():
        try:
            event = json.loads(line)
        except json.JSONDecodeError as error:
            # Skipping a line here would quietly shrink the evidence while the
            # artifact still reported the run as complete.
            raise ValueError(f'{path.name}: unreadable line: {error}') from error
        kind = event.get('event')
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
        elif kind == 'latency_target':
            trial['latency_targets'].append(event)
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
                      'reference_state', 'overload_outcome', 'observers_detached'):
            trial[kind] = event
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
    return trial


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', type=Path, required=True, help='a load.py --output directory')
    parser.add_argument('--output', type=Path, required=True, help='artifact JSON to write')
    parser.add_argument('--full-process-rows', action='append', default=['resources-'],
                        help='case name prefixes whose per-process census rows are kept whole')
    args = parser.parse_args()
    summary = json.loads((args.input / 'summary.json').read_text())
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
        # A file truncated at a line boundary parses cleanly and ends early, so
        # neither the missing-file nor the malformed-line check sees it. Without
        # this the trial would inherit `passed` from the summary while holding
        # no result at all.
        if trial['trial_result'] is None and trial['failure'] is None:
            raise SystemExit(
                f'{row["path"]} ends without a trial_result or a trial_failure; '
                f'summary.json records passed={row["passed"]} for it, which nothing '
                f'in the file supports')
        trial['case'] = row['case']
        trial['trial'] = row['trial']
        trial['passed'] = row['passed']
        trials.append(trial)
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
                               'which is what shows accumulation',
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
            'case_outcomes': 'fairness, reference_state, overload_outcome and '
                             'observers_detached are kept whole: they are what their '
                             'cases exist to produce, and a summary of them is not evidence',
            'not_retained': 'per-producer ledger rows (totalled), individual budget records, '
                            'and per-process rows for non-resource cases outside the closing census',
        },
        'trials': trials,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(artifact, sort_keys=True) + '\n')
    print(f'{args.output} {args.output.stat().st_size} bytes, {len(trials)} trials')


if __name__ == '__main__':
    main()
