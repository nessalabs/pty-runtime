"""ADR workload matrix. Reductions are explicitly smoke evidence only."""
MIB = 1024 * 1024

def cases(smoke=False):
    baseline = dict(sessions=64, active=16, rate=10*MIB, chunk=4093, observers=1,
                    cols=80, rows=24, raw=False, seconds=60, warmup=10)
    result = {}
    def add(name, **changes):
        result[name] = {**baseline, 'mode': 'attached', **changes}
    for mode in ('attached', 'detached', 'stalled-observer', 'stalled-sink', 'dominant'):
        add(mode, mode=mode)
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
    for raw in (True, False):
        for count in (1, 32, 128):
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
