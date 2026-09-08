#!/usr/bin/env python3
"""Drive real release_stress with watchdogs, source identity, and resource census.

No reduced run is acceptance. Full soak defaults to twelve hours inside the Rust
example. Samples describe this test process tree, not host-wide resource usage.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import queue
import shutil
import tempfile
import threading
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'guardian'))
from resources import process_costs


def tree(owner):
    output = subprocess.check_output(['ps', '-axo', 'pid=,ppid=,stat='], text=True)
    rows = {int(pid): (int(ppid), state) for pid, ppid, state in map(str.split, output.splitlines())}
    selected = {owner}
    while True:
        expanded = selected | {pid for pid, (parent, _) in rows.items() if parent in selected}
        if expanded == selected:
            break
        selected = expanded
    return {pid: rows[pid][1] for pid in selected if pid in rows}


def sample(owner, started, phase):
    members = tree(owner)
    costs = []
    vanished = []
    for pid in members:
        try:
            costs.extend(process_costs([pid]))
        except (FileNotFoundError, ProcessLookupError, AssertionError, subprocess.CalledProcessError):
            vanished.append(pid)
    return {'event': 'resources', 'elapsed_seconds': time.monotonic() - started,
            'phase': phase, 'processes': costs, 'vanished_during_sample': vanished,
            'zombies': [pid for pid, state in members.items() if state.startswith('Z')],
            'tree_process_count': len(members)}


def settled_sample(owner, started, phase):
    deadline = time.monotonic() + 10
    while len(tree(owner)) > 1 and time.monotonic() < deadline:
        time.sleep(0.02)
    result = sample(owner, started, phase)
    assert result['tree_process_count'] == 1, ('children retained at quiescence', result)
    assert not result['zombies'], result
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('mode', choices=['repetition', 'races', 'soak'])
    parser.add_argument('--binary', type=Path, default=Path('target/release/examples/release_stress'))
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--smoke', action='store_true')
    parser.add_argument('--parking-smoke', action='store_true')
    parser.add_argument('--restore-smoke', action='store_true')
    parser.add_argument('--sample-seconds', type=float, default=60)
    args = parser.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    image_directory = tempfile.TemporaryDirectory(prefix='pty-release-image-')
    executable = Path(image_directory.name) / 'release_stress'
    shutil.copy2(args.binary.resolve(), executable)
    source = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
    dirty = subprocess.check_output(['git', 'diff', '--binary'])
    inventory = subprocess.check_output(['git', 'ls-files', '--cached', '--others', '--exclude-standard'], text=True)
    digest = hashlib.sha256()
    for name in sorted(inventory.splitlines()):
        path = Path(name)
        if path.is_file() and path.suffix in ('.rs', '.toml', '.lock', '.py', '.c', '.h'):
            digest.update(name.encode() + b'\0' + path.read_bytes())
    args.smoke = args.smoke or args.parking_smoke or args.restore_smoke
    command = [str(executable), args.mode] + (['--restore-smoke'] if args.restore_smoke else ['--parking-smoke'] if args.parking_smoke else ['--smoke'] if args.smoke else [])
    started = time.monotonic()
    environment = {**os.environ, 'PTY_RELEASE_CENSUS': '1'}
    process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                               stderr=subprocess.STDOUT, text=True, bufsize=1, env=environment)
    lines = queue.Queue()
    def read_output():
        for line in process.stdout:
            lines.put(line)
    reader = threading.Thread(target=read_output, daemon=True)
    reader.start()
    samples = []
    complete = False
    last_progress = started
    next_sample = started + args.sample_seconds
    with args.output.open('w') as output:
        def record(value):
            output.write(json.dumps(value) + '\n')
            output.flush()
        record({'event': 'identity', 'platform': platform.platform(), 'machine': platform.machine(),
                'source_head': source, 'diff_sha256': hashlib.sha256(dirty).hexdigest(),
                'source_inventory_sha256': digest.hexdigest(),
                'binary_sha256': hashlib.sha256(executable.read_bytes()).hexdigest(),
                'command': command, 'smoke': args.smoke, 'owner_pid': process.pid,
                'warmup_seconds': 120, 'sample_seconds': args.sample_seconds})
        try:
            while process.poll() is None:
                if time.monotonic() - last_progress > 45:
                    raise TimeoutError('no completed operation/progress within45seconds')
                try:
                    line = lines.get(timeout=0.2)
                except queue.Empty:
                    line = None
                if line is not None:
                    try:
                        event = json.loads(line)
                    except json.JSONDecodeError:
                        record({'event': 'stderr', 'text': line.rstrip()})
                        continue
                    record(event)
                    last_progress = time.monotonic()
                    complete |= event.get('event') == 'complete'
                    if event.get('event') == 'checkpoint':
                        result = settled_sample(process.pid, started, event['phase'])
                        samples.append(result)
                        record(result)
                        process.stdin.write('continue\n')
                        process.stdin.flush()
                if time.monotonic() >= next_sample:
                    result = sample(process.pid, started, 'periodic')
                    samples.append(result)
                    record(result)
                    next_sample = time.monotonic() + args.sample_seconds
            reader.join(timeout=5)
            while not lines.empty():
                line = lines.get_nowait()
                try:
                    event = json.loads(line)
                    record(event)
                    complete |= event.get('event') == 'complete'
                except json.JSONDecodeError:
                    record({'event': 'stderr', 'text': line.rstrip()})
            assert process.returncode == 0 and complete, (process.returncode, complete)
            quiet = [value for value in samples if value['phase'] in ('baseline', 'final')]
            if args.mode == 'repetition':
                assert len(quiet) == 2, quiet
                before, after = (value['processes'][0]['fds'] for value in quiet)
                assert before == after, ('quiescent descriptor leak', before, after)
            warm = [value for value in samples if value['elapsed_seconds'] >= 120]
            record({'event': 'driver_complete', 'smoke': args.smoke, 'samples_after_warmup': len(warm),
                    'elapsed_seconds': time.monotonic() - started,
                    'memory_plateau': 'requires review of timeseries; no peak or plateau inferred from smoke'})
        except BaseException as error:
            record({'event': 'driver_failure', 'error': str(error)})
            if process.poll() is None:
                process.kill()  # Exact unreaped child; guardian owner EOF retains cleanup authority.
            process.wait(timeout=15)
            raise
        finally:
            process.stdin.close()
            process.stdout.close()
            image_directory.cleanup()


if __name__ == '__main__':
    main()
