#!/usr/bin/env python3
"""Decide whether a run's resources accumulate, from its retained series.

ADR 0002 asks the 12-hour soak for "stable resource plateaus after warm-up, no
accumulating children/descriptors/workers". That is a question about a *slope*,
not a magnitude, and a slope is measurable long before the thing it predicts
becomes visible. Eyeballing a twelve-hour plateau is the least sensitive way to
ask it: a leak of one descriptor per thousand operations is a straight line on
any timescale and a flat-looking chart on all of them.

Leak sanitizers do not answer it either. They find memory that became
unreachable. Retention — a map that gains an entry per session, a descriptor
held by a live session, a worker never joined — is perfectly reachable and
perfectly fine to a sanitizer, and is most of what this criterion is about.

So: fit a line, and report both whether the slope is distinguishable from zero
and whether it is large enough to matter. Neither alone is enough. A slope can
be statistically certain and physically irrelevant, and a large slope over three
samples can be noise.
"""
import argparse
import json
from pathlib import Path

# A slope this many standard errors from zero is not chance. Three is roughly
# the 99% mark for the sample counts a 60-second trial produces.
SIGNIFICANCE = 3.0


def slope(points):
    """Least-squares slope and its standard error, per second.

    Returns `None` when there is too little to fit: fewer than four samples, or
    no spread in time. Saying "not enough data" is the honest answer there, and
    is not the same as saying the slope is zero.
    """
    if len(points) < 4:
        return None
    times = [t for t, _ in points]
    values = [v for _, v in points]
    n = len(points)
    mean_t = sum(times) / n
    mean_v = sum(values) / n
    span = sum((t - mean_t) ** 2 for t in times)
    if span == 0:
        return None
    gradient = sum((t - mean_t) * (v - mean_v) for t, v in points) / span
    intercept = mean_v - gradient * mean_t
    residuals = [v - (intercept + gradient * t) for t, v in points]
    if n <= 2:
        return None
    variance = sum(r * r for r in residuals) / (n - 2)
    error = (variance / span) ** 0.5 if span else 0.0
    return {'per_second': gradient, 'standard_error': error,
            'significance': abs(gradient) / error if error else (0.0 if gradient == 0 else float('inf')),
            'samples': n, 'mean': mean_v,
            'seconds_observed': max(times) - min(times)}


def measurement_window(series, warmup_seconds):
    """The steady-state span, by phase marker where the run recorded one.

    A trial is a hump, not a plateau: sessions spawn, run, and are torn down.
    Fitting a line across the whole of it measures the teardown — on real data
    that read as -46 000 descriptors an hour, which is a trial ending, not a
    leak. The `measurement_start` and `measurement_end` checkpoints bound the
    part where the population is meant to be stable, so use them, and fall back
    to a warm-up offset only when they are absent.
    """
    marks = {row.get('phase'): row['monotonic_seconds'] for row in series}
    first = min((row['monotonic_seconds'] for row in series), default=0.0)
    if 'measurement_start' in marks and 'measurement_end' in marks:
        return marks['measurement_start'], marks['measurement_end'], 'phase markers'
    return first + warmup_seconds, float('inf'), 'warm-up offset'


def assess(series, warmup_seconds, metric, floor):
    """Judge one metric's series. `floor` is the growth per hour worth caring about."""
    opened, closed, how = measurement_window(series, warmup_seconds)
    started = min((row['monotonic_seconds'] for row in series), default=0)
    points = [(row['monotonic_seconds'] - started, row[metric])
              for row in series
              if row.get(metric) is not None
              and opened <= row['monotonic_seconds'] <= closed]
    fit = slope(points)
    if fit is None:
        return {'metric': metric, 'verdict': 'insufficient data', 'window': how,
                'samples': len(points), 'accumulating': None}
    per_hour = fit['per_second'] * 3600
    observed = fit['seconds_observed']
    # Judged inside the data. Extrapolating a noisy metric from sixty seconds to
    # an hour multiplies every wobble by sixty, which flagged four trials whose
    # RSS moved 0.3 MiB on a 443 MiB base.
    growth = fit['per_second'] * observed
    kind, bound = floor
    threshold = bound if kind == 'absolute' else abs(fit['mean']) * bound
    significant = fit['significance'] >= SIGNIFICANCE
    material = abs(growth) >= threshold
    stretch = 3600.0 / observed if observed else float('inf')
    return {'metric': metric,
            'growth_over_window': growth, 'window_seconds': observed,
            'threshold_over_window': threshold, 'threshold_kind': kind,
            'per_hour_extrapolated': per_hour,
            'extrapolation_factor': stretch,
            'per_hour_is_trustworthy': stretch <= TRUSTWORTHY_EXTRAPOLATION,
            'window': how,
            'significance': fit['significance'], 'samples': fit['samples'],
            'seconds_observed': observed, 'mean': fit['mean'],
            'statistically_distinguishable_from_zero': significant,
            'large_enough_to_matter': material,
            # Both, or it is not a finding. A certain slope of nothing is not a
            # leak, and a big slope over five noisy samples is not evidence.
            'accumulating': bool(significant and material and growth > 0),
            'verdict': ('accumulating' if significant and material and growth > 0
                        else 'flat' if not significant
                        else 'moving but too little to matter' if not material
                        else 'decreasing')}


# What counts as growth *within the observed window*, which is where the
# evidence is. Counted things are absolute: gaining a descriptor and not giving
# it back is a leak however slowly it happens. Memory is relative, because RSS
# wanders with allocator behaviour and an absolute byte floor would flag any
# busy process.
FLOORS = {'fds': ('absolute', 1.0), 'threads': ('absolute', 1.0),
          'tree_processes': ('absolute', 1.0), 'rss_bytes': ('relative', 0.01)}

# An hour projected from a minute is a sixtyfold extrapolation. Past this, the
# per-hour figure is reported but is not evidence of anything.
TRUSTWORTHY_EXTRAPOLATION = 10.0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--artifact', type=Path, required=True,
                        help='an archive.py artifact')
    parser.add_argument('--warmup-seconds', type=float, default=120.0)
    parser.add_argument('--case', action='append', help='restrict to these cases')
    args = parser.parse_args()
    artifact = json.loads(args.artifact.read_text())
    findings = []
    for trial in artifact['trials']:
        if args.case and trial['case'] not in args.case:
            continue
        series = trial.get('resource_series') or []
        assessed = [assess(series, args.warmup_seconds, metric, floor)
                    for metric, floor in sorted(FLOORS.items())]
        findings.append({'case': trial['case'], 'trial': trial['trial'],
                         'metrics': assessed,
                         'accumulating': [row['metric'] for row in assessed if row['accumulating']],
                         'undecided': [row['metric'] for row in assessed
                                       if row['accumulating'] is None]})
    accumulating = [row for row in findings if row['accumulating']]
    undecided = [row for row in findings if row['undecided']]
    # An empty selection is not a flat resource profile. Neither is a trial with
    # too few samples to fit. Anything using this as a gate would otherwise read
    # "no evidence" as "no accumulation", which is the one mistake a check like
    # this must not make.
    verdict = 0 if findings and not accumulating and not undecided else (
        1 if accumulating else 2)
    print(json.dumps({'warmup_seconds': args.warmup_seconds,
                      'significance_threshold': SIGNIFICANCE,
                      'floors_within_window': {k: list(v) for k, v in FLOORS.items()},
                      'trials': findings,
                      'trials_accumulating': len(accumulating),
                      'trials_undecided': len(undecided),
                      'exit_status': {'0': 'every selected metric decided and flat',
                                      '1': 'something accumulates',
                                      '2': 'insufficient evidence: an undecided metric, '
                                           'or a selection that matched no trial'}[str(verdict)],
                      'scope': 'A flat verdict bounds accumulation over the observed '
                               'window at the floor; it does not prove a twelve-hour '
                               'plateau, and a short window cannot see a slope whose '
                               'onset is later than it. Per-hour figures are '
                               'extrapolations and are marked untrustworthy beyond a '
                               'tenfold stretch.'},
                     indent=2, sort_keys=True))
    raise SystemExit(verdict)


if __name__ == '__main__':
    main()
