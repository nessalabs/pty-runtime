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
    # How much of the movement the line actually accounts for. Significance says
    # the slope is not chance; it says nothing about whether a line describes the
    # series at all. A metric that alternates between two values will hand a
    # confident slope to any fit whose window catches one value more often at one
    # end, and that is a duty cycle shifting, not a resource accumulating.
    total = sum((v - mean_v) ** 2 for _, v in points)
    explained = 1.0 - sum(r * r for r in residuals) / total if total else 1.0
    return {'per_second': gradient, 'standard_error': error,
            'variance_explained': explained,
            'observed_range': max(values) - min(values),
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


def population_is_comparable(points):
    """Whether the totals were taken over a stable set of processes.

    A census that loses a process reports totals over the survivors. That is
    harmless when the same number is lost every time - a constant omission
    shifts every point equally and leaves the slope alone - and ruinous when the
    number grows, because the totals then fall for a reason that has nothing to
    do with the resource being measured, and a real leak can be masked as flat.

    In the 55-minute soak 326 of 327 samples were degraded, always by the same
    four transient probe processes, with the measured count 193 in 325 of them.
    That is the harmless case, and this is how it is told from the other one.
    """
    fit = slope(points)
    if fit is None:
        return None, fit
    return fit['significance'] < SIGNIFICANCE, fit


def judge(points, floor):
    """Fit one series and say whether it grows, and whether that growth matters."""
    fit = slope(points)
    if fit is None:
        return None
    observed = fit['seconds_observed']
    # Judged inside the data. Extrapolating a noisy metric from sixty seconds to
    # an hour multiplies every wobble by sixty, which flagged four trials whose
    # RSS moved 0.3 MiB on a 443 MiB base.
    growth = fit['per_second'] * observed
    kind, bound = floor
    threshold = bound if kind == 'absolute' else abs(fit['mean']) * bound
    significant = fit['significance'] >= SIGNIFICANCE
    material = abs(growth) >= threshold
    linear = fit['variance_explained'] >= LINEARITY
    stretch = 3600.0 / observed if observed else float('inf')
    # Significant, material, and a line that does not describe the series: the
    # window cannot settle it either way, so say so rather than picking the
    # answer the slope's sign happens to give.
    undecided = significant and material and not linear
    return {'growth_over_window': growth, 'window_seconds': observed,
            'variance_explained': fit['variance_explained'],
            'observed_range': fit['observed_range'],
            'a_line_describes_this_series': linear,
            'threshold_over_window': threshold, 'threshold_kind': kind,
            'per_hour_extrapolated': fit['per_second'] * 3600,
            'extrapolation_factor': stretch,
            'per_hour_is_trustworthy': stretch <= TRUSTWORTHY_EXTRAPOLATION,
            'significance': fit['significance'], 'samples': fit['samples'],
            'seconds_observed': observed, 'mean': fit['mean'],
            'statistically_distinguishable_from_zero': significant,
            'large_enough_to_matter': material,
            # Both, or it is not a finding. A certain slope of nothing is not a
            # leak, and a big slope over five noisy samples is not evidence.
            'accumulating': None if undecided else bool(significant and material and growth > 0),
            'verdict': ('inconclusive: a line does not describe this series' if undecided
                        else 'accumulating' if significant and material and growth > 0
                        else 'flat' if not significant
                        else 'moving but too little to matter' if not material
                        else 'decreasing')}


def assess(series, warmup_seconds, metric, floor):
    """Judge one metric's series. `floor` is the growth per hour worth caring about.

    Two bases, because neither alone is safe.

    `stable_cohort` totals only the processes present in every census of the
    window, so the population behind consecutive points is identical by
    construction. That closes the hole a count-only comparability check leaves
    open: a costly process leaving while a cheap one arrives keeps
    `measured_processes` perfectly flat and moves the totals anyway, which can
    mask a real slope.

    `all_measured` totals whatever each census could measure. It is the only
    basis that can see a leak living in the churn itself, and it is trusted only
    while the measured count is not itself moving.

    A metric accumulates if either decidable basis says so.
    """
    opened, closed, how = measurement_window(series, warmup_seconds)
    started = min((row['monotonic_seconds'] for row in series), default=0)
    rows = [row for row in series if opened <= row['monotonic_seconds'] <= closed]
    degraded = sum(1 for row in rows if row.get('unavailable_pids'))

    def points(field):
        return [(row['monotonic_seconds'] - started, row[field])
                for row in rows if row.get(field) is not None]

    sizes = {row['cohort_processes'] for row in rows
             if row.get('cohort_processes') is not None}
    # A cohort is an intersection, so its size is the same in every sample it
    # covers. A varying size means these rows are not what this claims to read.
    cohort = (judge(points('cohort_' + metric), floor)
              if len(sizes) == 1 and next(iter(sizes)) > 0 else None)
    counted = points('measured_processes')
    comparable, population = population_is_comparable(counted)
    measured = judge(points(metric), floor)
    counts_the_population = metric in POPULATION_METRICS

    bases = {}
    if cohort is not None:
        bases['stable_cohort'] = {
            **cohort, 'population_comparable': True, 'processes': next(iter(sizes)),
            'population_basis': 'the same processes in every sample, by construction'}
    if measured is not None:
        bases['all_measured'] = {
            **measured,
            'population_comparable': True if counts_the_population else comparable,
            'population_basis': (
                'a direct count of the process tree, not a sum over the processes a '
                'census could measure, so its own growth does not disqualify it'
                if counts_the_population
                else 'totals over the processes each census could measure'),
            'measured_process_slope_per_hour': population['per_second'] * 3600 if population else None,
            'measured_process_significance': population['significance'] if population else None}
    # `None` is "no count series to check", which is not evidence of movement.
    decisive = [name for name, row in bases.items()
                if row['population_comparable'] is not False]
    common = {'metric': metric, 'window': how, 'degraded_samples': degraded,
              'measured_population_stable': comparable, 'bases': bases}
    if not decisive:
        if not bases:
            return {**common, 'verdict': 'insufficient data', 'accumulating': None,
                    'samples': len(points(metric))}
        return {**common, 'verdict': 'unusable: the measured population moved',
                'accumulating': None, 'samples': bases['all_measured']['samples'],
                'measured_process_slope_per_hour':
                    bases['all_measured']['measured_process_slope_per_hour'],
                'measured_process_significance':
                    bases['all_measured']['measured_process_significance']}
    name = 'stable_cohort' if 'stable_cohort' in decisive else 'all_measured'
    accumulating = any(bases[basis]['accumulating'] for basis in decisive)
    # A basis that could not decide leaves the metric undecided unless another
    # one found accumulation: "we could not tell" must never read as "flat".
    if not accumulating and any(bases[basis]['accumulating'] is None for basis in decisive):
        accumulating = None
    return {**common, **bases[name], 'basis': name, 'decisive_bases': decisive,
            'accumulating': accumulating,
            'verdict': ('accumulating' if accumulating
                        else bases[name]['verdict'] if accumulating is False
                        else next(bases[basis]['verdict'] for basis in decisive
                                  if bases[basis]['accumulating'] is None))}


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

# A line has to explain more of the movement than everything else put together
# before a slope drawn through it is called a trend. Below this the fit is
# reported as undecided rather than as flat *or* as accumulating: undecided is
# the honest answer for a window that cannot settle the question, and it exits
# non-zero, so nothing reads it as a pass.
#
# A 55-minute soak produced a descriptor slope of 2.62 over the window at
# significance 4.28 - comfortably "accumulating" on both counts - from a series
# that simply alternates between 1,864 and 1,873 descriptors as a transient
# probe comes and goes. The line explained 2.8% of the variance. A clean
# synthetic leak explains 99.8%.
LINEARITY = 0.5

# Metrics that *are* the population rather than a sum taken over it.
#
# `tree_processes` is the census's own walk of the process tree, so it does not
# depend on which processes could be measured, and the comparability gate must
# not apply to it. Gating it was worse than redundant: accumulating children
# raise the measured count and the tree count together, so the growth
# disqualified itself and the tool answered "unusable: the measured population
# moved" to precisely the leak ADR 0002 names it to find. There is no cohort
# analogue either — a cohort has a fixed size by construction — so that verdict
# left no basis at all.
POPULATION_METRICS = ('tree_processes',)


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
                      'degraded_samples': 'A census that loses a process totals the '
                                          'survivors. That is reported per metric, and a '
                                          'metric whose measured population is itself '
                                          'moving is refused rather than fitted.',
                      'bases': 'Each metric is fitted over a fixed cohort of processes '
                               'present in every census of the window, and over the totals '
                               'of whatever each census could measure. The cohort is '
                               'comparable between samples by construction, so process '
                               'turnover cannot mask a slope in it; the totals are the only '
                               'basis that can see a leak in the churn itself, and are used '
                               'only while the measured count is not itself moving. A '
                               'metric accumulates if either decidable basis says so. '
                               'Artifacts written before retention version 4 carry no '
                               'cohort series, so only the totals basis is available.',
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
