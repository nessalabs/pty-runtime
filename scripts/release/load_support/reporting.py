"""Aggregate recorded target evidence without conflating it with trial correctness."""
import json

LATENCY_BOUNDARIES = ('InputDispatch', 'RawOutput', 'ProjectedOutput', 'ResizeDispatch', 'CancelDispatch')


def verdict(values):
    if False in values:
        return False
    return True if values and all(value is True for value in values) else None


def measurement_verdict(observed, ceiling, failures=0, unavailable=0):
    if failures > 0 or (observed is not None and observed > ceiling):
        return False
    if observed is None or unavailable > 0:
        return None
    return True


def trial_targets(path, config):
    latency = {}
    idle = {}
    with path.open() as source:
        for line in source:
            event = json.loads(line)
            if event.get('event') == 'latency_target':
                latency[event['boundary']] = event
            elif event.get('event') == 'idle_cpu_target':
                idle = event
    return targets_from_events(latency, idle, config)


def recompute_target(event, ceiling_key, observed_key):
    """The verdict its own numbers support, ignoring the recorded `passed`.

    A target record carries both the measurement and the conclusion drawn from
    it, and nothing compared them: a record observing 224 ms against a 20 ms
    ceiling while claiming `passed: true` was republished as a pass. The bit is
    a convenience; the numbers are the evidence.
    """
    if not event:
        return None
    return measurement_verdict(event.get(observed_key), event.get(ceiling_key),
                               event.get('failures', 0), event.get('unavailable', 0))


def disagreeing_targets(latency, idle):
    """Target records whose stated verdict their own measurements do not support."""
    wrong = {}
    for name, event in sorted(latency.items()):
        expected = recompute_target(event, 'target_p99_us', 'observed_p99_us')
        if event.get('passed') != expected:
            wrong[name] = {'recorded': event.get('passed'), 'measurements_give': expected,
                           'observed_p99_us': event.get('observed_p99_us'),
                           'target_p99_us': event.get('target_p99_us')}
    if idle:
        expected = recompute_target(idle, 'target_core_percent', 'measured_core_percent')
        if idle.get('passed') != expected:
            wrong['idle_cpu'] = {'recorded': idle.get('passed'), 'measurements_give': expected,
                                 'measured_core_percent': idle.get('measured_core_percent'),
                                 'target_core_percent': idle.get('target_core_percent')}
    return wrong


def targets_from_events(latency, idle, config):
    """The per-trial target verdicts, from the trial's own target records.

    Split out of `trial_targets` so the archiver can recompute a summary row
    from the records it retained and refuse a summary that disagrees with them,
    rather than republishing whatever the driver wrote. One implementation, so
    the check cannot drift from the thing it checks.
    """
    expected = [name for name in LATENCY_BOUNDARIES
                if not (name == 'ProjectedOutput' and config['raw'])] if config['active'] > 0 else []
    missing = [name for name in expected
               if latency.get(name, {}).get('passed') is None
               or latency.get(name, {}).get('measurement_complete') is False]
    return dict(latency_targets_applicable=bool(expected),
                latency_targets_passed=verdict([latency.get(name, {}).get('passed') for name in expected]),
                latency_targets_missing=missing,
                latency_measurements_complete=not missing if expected else None,
                idle_cpu_target_applicable=config['mode'] == 'idle',
                idle_cpu_target_passed=idle.get('passed'),
                idle_cpu_measurement_complete=idle.get('measurement_complete', idle.get('passed') is not None),
                idle_cpu_acceptance_duration=idle.get('acceptance_duration'))


def rollup(results):
    aggregate = {}
    for name, prefix in [('latency', 'latency_targets'), ('idle_cpu', 'idle_cpu_target')]:
        applicable = [row for row in results if row[prefix + '_applicable']]
        values = [row[prefix + '_passed'] for row in applicable]
        aggregate[name] = dict(applicable_trials=len(values),
                               passed_trials=sum(value is True for value in values),
                               failed_trials=sum(value is False for value in values),
                               unmeasured_trials=sum(value is None for value in values),
                               all_applicable_passed=verdict(values))
        complete_key = 'latency_measurements_complete' if name == 'latency' else 'idle_cpu_measurement_complete'
        aggregate[name]['incomplete_trials'] = sum(row[complete_key] is not True for row in applicable)
        if name == 'idle_cpu':
            aggregate[name]['acceptance_duration_trials'] = sum(
                row['idle_cpu_acceptance_duration'] is True for row in applicable)
    return aggregate
