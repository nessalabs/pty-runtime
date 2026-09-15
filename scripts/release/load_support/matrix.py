"""ADR workload matrix. Reductions are explicitly smoke evidence only."""
MIB = 1024 * 1024

# Runnable by name, and deliberately not part of the default selection.
#
# ADR 0002 asks for 500 sessions "on a host with sufficient PTY/process
# capacity", and LOAD.md says that qualification is host-dependent and outside
# this bounded 128-session matrix. The raw case needs 1,501 processes and
# ~14,500 descriptors, which a common host does not have; the projected case
# cannot pass on any host at the shipped defaults, because `resident_bytes` is
# 1 GiB and each projected session reserves 8 MiB, admitting exactly 128.
#
# Selecting them by default made the documented `load.py --output ...` command
# record a failed trial and exit non-zero everywhere, which turns the run's
# pass bit from a result into noise.
HOST_DEPENDENT = ('resources-raw-500', 'resources-projected-500')

# The fixture's own defaults for the options the matrix does not state.
#
# `staging_slots` is the per-session parser-output queue depth, and it is the
# quantity Experiment 0006 exists to sweep - yet a default run recorded it
# nowhere, because the matrix does not pass it and the retained configuration
# therefore does not mention it. Stated here so the archiver has something to
# check the fixture's reported depth against; if the fixture's default changes,
# every projected measurement moves and the archiver refuses the run rather
# than publishing it as though nothing had.
FIXTURE_DEFAULTS = {'staging_slots': 256, 'producer_bytes': 8589934592}


def default_selection(smoke=False):
    """The bounded matrix: every case except the host-dependent ones."""
    return [name for name in cases(smoke) if name not in HOST_DEPENDENT]

def cases(smoke=False):
    baseline = dict(sessions=64, active=16, rate=10*MIB, chunk=4093, observers=1,
                    cols=80, rows=24, raw=False, seconds=60, warmup=10)
    result = {}
    def add(name, **changes):
        result[name] = {**baseline, 'mode': 'attached', **changes}
    # `detached` never attaches an observer; `detaching` attaches and then drops
    # them all half way through, which is the transition ADR 0004 asks about and
    # the one nothing else in this matrix exercises.
    for mode in ('attached', 'detached', 'detaching', 'reference',
                 'stalled-observer', 'stalled-sink', 'dominant'):
        add(mode, mode=mode)
    add('capacity-projected', mode='saturation', rate=0)
    add('capacity-raw', mode='saturation', rate=0, raw=True)
    add('128-active', sessions=128, active=128)
    add('128-mixed', sessions=128, active=32)
    for rate in (1, 20, 40):
        add(f'rate-{rate}MiB', rate=rate*MIB)
    for chunk in (1, 64, 65536):
        add(f'chunk-{chunk}', chunk=chunk)
    for observers in (4, 16):
        add(f'observers-{observers}', observers=observers)
    for cols, rows in ((160, 48), (240, 80)):
        add(f'grid-{cols}x{rows}', cols=cols, rows=rows)
    add('idle-64', mode='idle', active=0, rate=0, raw=True, observers=0)
    # 500 is defined here so `--case resources-raw-500` can run it; it is left
    # out of `default_selection`.
    for raw in (True, False):
        for count in (1, 32, 128, 500):
            add(f'resources-{"raw" if raw else "projected"}-{count}', mode='idle',
                sessions=count, active=0, rate=0, raw=raw, observers=0)
    if smoke:
        for value in result.values():
            value['sessions'] = min(value['sessions'], 4)
            value['active'] = min(value['active'], 2)
            value['rate'] = min(value['rate'], MIB)
            value['seconds'] = 2
            value['warmup'] = 1
    return result
