#!/usr/bin/env python3
"""Measure persistent helper costs separately from workload and runtime costs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import time
from probe import Session


def process_costs(pids):
    if platform.system() == 'Linux':
        rows = []
        for pid in pids:
            root = Path('/proc') / str(pid)
            stat = (root / 'stat').read_text().rsplit(')', 1)[1].split()
            memory = {}
            for line in (root / 'smaps_rollup').read_text().splitlines():
                key, _, value = line.partition(':')
                if key in ('Rss', 'Pss'):
                    memory[key] = int(value.split()[0]) * 1024
            rows.append({'pid': pid, 'rss_bytes': memory['Rss'], 'pss_bytes': memory['Pss'],
                         'cpu_seconds': (int(stat[11]) + int(stat[12])) / os.sysconf('SC_CLK_TCK'),
                         'fds': len(list((root / 'fd').iterdir())),
                         'threads': len(list((root / 'task').iterdir()))})
        return rows
    selector = ','.join(map(str, pids))
    output = subprocess.check_output(['ps', '-p', selector, '-o', 'pid=,rss=,time='], text=True)
    fds = {}
    current = None
    inventory = subprocess.run(['lsof', '-Fpf', '-p', selector], capture_output=True, text=True)
    if inventory.returncode != 0:
        raise ProcessLookupError(
            f'lsof process census pids={pids} exit={inventory.returncode} '
            f'stderr={inventory.stderr!r} stdout={inventory.stdout!r}')
    for line in inventory.stdout.splitlines():
        if line.startswith('p'):
            current = int(line[1:])
            fds[current] = 0
        elif line.startswith('f') and line[1:].isdigit():
            fds[current] += 1
    missing_fds = sorted({int(line.split()[0]) for line in output.splitlines()} - fds.keys())
    if missing_fds:
        raise ProcessLookupError(f'lsof process census missing_pids={missing_fds} requested_pids={pids} stdout={inventory.stdout!r}')
    rows = []
    for line in output.splitlines():
        pid, rss, elapsed = line.split()
        minutes, seconds = elapsed.split(':')
        rows.append({'pid': int(pid), 'rss_bytes': int(rss) * 1024, 'pss_bytes': None,
                     'cpu_seconds': int(minutes) * 60 + float(seconds),
                     'fds': fds[int(pid)], 'threads': None})
    missing = sorted(set(pids) - {row['pid'] for row in rows})
    if missing:
        raise ProcessLookupError(f'ps process census missing_pids={missing} requested_pids={pids} stdout={output!r}')
    return rows


def measure(helper, count, seconds):
    sessions = []
    launch = time.monotonic()
    try:
        pids = []
        for _ in range(count):
            session = Session(helper, 'exec /bin/sleep 300')
            sessions.append(session)
            _, guardian, sentinel, _ = session.admitted()
            pids.extend([guardian, sentinel])
        launch_seconds = time.monotonic() - launch
        before = process_costs(pids)
        sample_start = time.monotonic()
        time.sleep(seconds)
        after = process_costs(pids)
        elapsed = time.monotonic() - sample_start
        cpu = sum(row['cpu_seconds'] for row in after) - sum(row['cpu_seconds'] for row in before)
        return {'sessions': count, 'persistent_helpers': len(pids), 'launch_seconds': launch_seconds,
                'idle_sample_seconds': elapsed, 'helper_cpu_seconds': cpu,
                'helper_cpu_core_percent': cpu / elapsed * 100,
                'helper_rss_bytes': sum(row['rss_bytes'] for row in after),
                'helper_pss_bytes': sum(row['pss_bytes'] for row in after) if platform.system() == 'Linux' else None,
                'helper_fds': sum(row['fds'] for row in after), 'processes': after}
    finally:
        for session in sessions:
            session.close()


def measure_adapter(adapter, count, seconds):
    process = subprocess.Popen([str(adapter), str(count)], stdin=subprocess.PIPE,
                               stdout=subprocess.PIPE, text=True)
    try:
        assert process.stdout.readline().strip() == f'baseline {process.pid}'
        baseline = process_costs([process.pid])[0]
        launch = time.monotonic()
        process.stdin.write('start\n')
        process.stdin.flush()
        ready = process.stdout.readline().split()
        assert ready[0] == 'ready', ready
        launch_seconds = time.monotonic() - launch
        workloads = list(map(int, ready[1:]))
        assert len(workloads) == count
        rows = subprocess.check_output(['ps', '-axo', 'pid=,ppid='], text=True)
        parents = {int(pid): int(parent) for pid, parent in map(str.split, rows.splitlines())}
        helpers = [pid for workload in workloads for pid in [parents[workload], parents[parents[workload]]]]
        assert len(set(helpers)) == 2 * count
        pids = [process.pid, *helpers, *workloads]
        before = process_costs(pids)
        started = time.monotonic()
        time.sleep(seconds)
        after = process_costs(pids)
        elapsed = time.monotonic() - started
        costs = {row['pid']: row for row in after}
        result = {'sessions': count, 'persistent_helpers': len(helpers), 'launch_seconds': launch_seconds,
                  'idle_sample_seconds': elapsed, 'owner_baseline': baseline, 'owner_live': costs[process.pid],
                  'helpers': [costs[pid] for pid in helpers], 'workloads': [costs[pid] for pid in workloads],
                  'aggregate_cpu_seconds': sum(row['cpu_seconds'] for row in after) - sum(row['cpu_seconds'] for row in before)}
        process.stdin.write('close\n')
        process.stdin.flush()
        assert process.stdout.readline().strip() == 'closed'
        result['owner_after_shutdown'] = process_costs([process.pid])[0]
        process.stdin.close()
        assert process.wait(timeout=10) == 0
        return result
    finally:
        if process.poll() is None:
            process.stdin.close()
            process.wait(timeout=15)


def main():
    parser = argparse.ArgumentParser()
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument('--helper', type=Path)
    source.add_argument('--adapter', type=Path)
    parser.add_argument('--seconds', type=float, default=5)
    args = parser.parse_args()
    assert args.seconds > 0
    identity = {'platform': platform.platform(), 'machine': platform.machine(),
                'image_sha256': hashlib.sha256((args.helper or args.adapter).read_bytes()).hexdigest(),
                'scope': 'default packaged process adapter and sleeping workloads; native/projection excluded; PSS unavailable on macOS; transient anchors excluded' if args.adapter else 'persistent S/G only; workload/runtime excluded; PSS unavailable on macOS; transient anchors excluded'}
    for count in [1, 32, 128]:
        result = measure_adapter(args.adapter, count, args.seconds) if args.adapter else measure(args.helper, count, args.seconds)
        print(json.dumps({**identity, **result}), flush=True)


if __name__ == '__main__':
    main()
