"""Process-wide physical costs, separate from logical admission quotas."""
import subprocess
import platform
from collections import Counter
import sys
import time
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'guardian'))
from resources import process_costs


def members(owner):
    rows = subprocess.check_output(['ps', '-axo', 'pid=,ppid=,stat='], text=True)
    table = {int(pid): (int(parent), state) for pid, parent, state in map(str.split, rows.splitlines())}
    selected = {owner}
    while True:
        expanded = selected | {pid for pid, (parent, _) in table.items() if parent in selected}
        if expanded == selected:
            return {pid: table[pid] for pid in selected if pid in table}
        selected = expanded


def sample(owner, workloads, phase):
    table = members(owner)
    started = time.monotonic()
    # Batch on Darwin: per-process lsof has prohibitive observer overhead at 128 sessions.
    # A process that exits mid-census is reported, not raised: it is ordinary at
    # this scale, and failing the sample would discard every other process too.
    rows, unavailable = process_costs(sorted(table))
    thread_counts = {}
    if platform.system() == 'Darwin':
        result = subprocess.run(['ps', '-M', '-p', ','.join(map(str, table))], capture_output=True, text=True)
        if result.returncode == 0:
            thread_counts = Counter(int(fields[0] if fields[0].isdigit() else fields[1]) for line in result.stdout.splitlines() if len(fields := line.split()) > 1 and (fields[0].isdigit() or fields[1].isdigit()))
    for row in rows:
        if row['pid'] in thread_counts:
            row['threads'] = thread_counts[row['pid']]
        row['category'] = 'owner' if row['pid'] == owner else 'workload_fixture' if row['pid'] in workloads else 'guardian_helper'
    return dict(event='physical_resources', phase=phase, monotonic_seconds=started,
                collection_seconds=time.monotonic()-started, processes=rows,
                unavailable_pids=unavailable,
                unavailable_reason='sampled process exited during the census' if unavailable else None,
                wakeup_counts=None, wakeup_counts_reason='No portable per-process wakeup counter available from this collector',
                tree_processes=len(table), zombies=[pid for pid, (_, state) in table.items() if state.startswith('Z')])


def identify(row):
    """A process, not a number.

    Subtracting one census from another by pid alone credits a recycled number's
    new occupant with its predecessor's CPU, which on a long run is a negative
    delta out of nowhere. Where the collector can name the incarnation, use it.
    """
    return (row['pid'], row.get('start_ticks'))


def cpu_delta(before, after):
    seconds = after['monotonic_seconds'] - before['monotonic_seconds']
    original = {identify(row): row for row in before['processes']}
    final = {identify(row): row for row in after['processes']}
    result = {}
    for category in ('owner', 'workload_fixture', 'guardian_helper'):
        rows = [row for row in after['processes']
                if row['category'] == category and identify(row) in original]
        cpu = sum(row['cpu_seconds']-original[identify(row)]['cpu_seconds'] for row in rows) if rows else None
        result[category] = dict(cpu_seconds=cpu, core_percent=cpu/seconds*100 if cpu is not None else None,
                                matched_processes=len(rows))
    return dict(event='cpu_interval', seconds=seconds, categories=result,
                includes_census_overhead=True,
                complete_process_tree_accounting=False,
                measurement_scope='CPU deltas for PIDs present in both snapshots; processes created and exited between snapshots are unmeasured',
                unmatched_before_pids=sorted(pid for pid, _ in original.keys() - final.keys()),
                unmatched_after_pids=sorted(pid for pid, _ in final.keys() - original.keys()),
                unavailable_pids=sorted(set(before.get('unavailable_pids', [])) | set(after.get('unavailable_pids', []))))
