#!/usr/bin/env python3
"""Run and retain the bounded acceptance workload; this is not the full G4 suite."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import signal
import subprocess
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]


def identity():
    paths = []
    for folder in ['src', 'crates', 'tests', 'scripts', 'helpers', 'experiments', 'examples']:
        for directory, children, names in os.walk(ROOT / folder):
            children[:] = [child for child in children if child not in {'target', '.git', '__pycache__', 'work'}]
            paths.extend(Path(directory) / name for name in names
                         if Path(name).suffix in {'.rs', '.c', '.h', '.toml', '.py', '.lock', '.zig', '.patch', '.json', '.sh'})
    paths.extend([ROOT / 'Cargo.toml', ROOT / 'Cargo.lock', Path(__file__).resolve()])
    digest = hashlib.sha256()
    for path in sorted(set(paths)):
        digest.update(str(path.relative_to(ROOT)).encode() + b'\0')
        digest.update(path.read_bytes())
    return digest.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=ROOT / 'work/runtime-performance' /
                        (time.strftime('%Y%m%dT%H%M%SZ', time.gmtime()) + '-' + uuid.uuid4().hex[:8]))
    args = parser.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.mkdir(exist_ok=False)
    print(f'Evidence directory: {args.output}', flush=True)
    before = identity()
    command = ['cargo', 'test', '--locked', '--release', '--test', 'runtime_performance',
               '--features', 'ghostty', '--', '--ignored', '--nocapture', '--test-threads=1']
    metadata = {
        'scope': 'one 4 MiB/session trial at 1 and 16 sessions; not full G4 repeats or soak',
        'platform': platform.platform(), 'machine': platform.machine(),
        'revision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'source_sha256_before': before, 'command': command,
        'rustc': subprocess.check_output(['rustc', '--version'], cwd=ROOT, text=True).strip(),
        'started_unix_seconds': time.time(),
        'rss_scope': 'test process point samples; excludes separate helper/producer processes; not peak',
    }
    started = time.monotonic()
    child = subprocess.Popen(command, cwd=ROOT, text=True, stdout=subprocess.PIPE,
                             stderr=subprocess.STDOUT, start_new_session=True,
                             env=os.environ.copy())
    try:
        output, _ = child.communicate(timeout=600)
        code, timed_out = child.returncode, False
    except subprocess.TimeoutExpired as error:
        # Stop Cargo and its test process together; killing only Cargo leaves the
        # measurement running. PTY helper owner-EOF cleanup remains its own contract.
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        try:
            output, _ = child.communicate(timeout=10)
        except subprocess.TimeoutExpired as drain_error:
            output = drain_error.stdout or error.stdout or ''
            if child.stdout is not None:
                child.stdout.close()
            child.wait(timeout=10)
        if isinstance(output, bytes):
            output = output.decode('utf-8', errors='replace')
        code, timed_out = 124, True
    print(output, end='')
    (args.output / 'raw.txt').write_text(output)
    metadata.update(exit_code=code, timed_out=timed_out,
                    command_elapsed_seconds=time.monotonic() - started,
                    source_sha256_after=identity())
    metadata['source_stable'] = before == metadata['source_sha256_after']
    (args.output / 'metadata.json').write_text(json.dumps(metadata, indent=2) + '\n')
    records = [json.loads(line.split('PERFORMANCE_JSON ', 1)[1])
               for line in output.splitlines() if 'PERFORMANCE_JSON ' in line]
    (args.output / 'records.jsonl').write_text(''.join(json.dumps(row) + '\n' for row in records))
    if code:
        raise SystemExit(code)
    if len(records) != 2 or [row['sessions'] for row in records] != [1, 16]:
        raise SystemExit('Missing required 1/16 session records')
    for row in records:
        if (not row['success'] or row['build'] != 'release'
                or row['raw_bytes_validated'] != row['parser_processed_bytes']
                or row['raw_bytes_validated'] <= row['sessions'] * row['payload_bytes_per_session']):
            raise SystemExit('Invalid correctness evidence in performance record')
    if not metadata['source_stable']:
        raise SystemExit('Sources changed during measurement; rerun at a stable revision')


if __name__ == '__main__':
    main()
