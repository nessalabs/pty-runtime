#!/usr/bin/env python3
"""Measure the bounded protocol workloads, preserving five trials and exact source identity."""
import argparse
import json
import os
import signal
from pathlib import Path
import platform
import subprocess
import time
import uuid

from performance import ROOT, identity


def _cleanup_interrupted_benchmark(child):
    """Finish the owned Cargo lifetime despite repeated cancellation, within ten seconds."""
    deadline = time.monotonic() + 10

    def stop_group():
        # Cargo waits for its benchmark. Once reaped, its cached PID no longer
        # proves group ownership and must never be used for a group signal.
        if child.returncode is None:
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass

    def remaining():
        return max(0, deadline - time.monotonic())

    # Cancellation returns no captured output. Close directly: timeout cleanup
    # may already have closed a pipe, making communicate() unsafe to re-enter.
    operations = (stop_group, child.stdout.close, child.stderr.close,
                  lambda: child.wait(timeout=remaining()))
    failures = []
    for operation in operations:
        while True:
            try:
                operation()
                break
            except KeyboardInterrupt:
                if remaining() == 0:
                    failures.append('cleanup repeatedly interrupted past its deadline')
                    break
            except (OSError, subprocess.TimeoutExpired) as failure:
                failures.append(str(failure))
                break
    return failures


def run_benchmark(command, timeout_seconds=300):
    """Run Cargo and its benchmark in one lifetime, returning bounded timeout evidence."""
    child = subprocess.Popen(command, cwd=ROOT, stdout=subprocess.PIPE,
                             stderr=subprocess.PIPE, text=True, start_new_session=True)
    try:
        try:
            output, errors = child.communicate(timeout=timeout_seconds)
            code = child.returncode
        except subprocess.TimeoutExpired as failure:
            # Cargo owns a benchmark child. Stop their whole process group on timeout.
            if child.returncode is None:
                try:
                    os.killpg(child.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            try:
                output, errors = child.communicate(timeout=10)
            except subprocess.TimeoutExpired:
                output, errors = failure.stdout or b'', failure.stderr or b''
                child.stdout.close()
                child.stderr.close()
                child.wait(timeout=10)
            output = output.decode(errors='replace') if isinstance(output, bytes) else output
            errors = errors.decode(errors='replace') if isinstance(errors, bytes) else errors
            code = 124
    except KeyboardInterrupt as interruption:
        failures = _cleanup_interrupted_benchmark(child)
        if failures:
            interruption.cleanup_failures = tuple(failures)
        raise
    return output, errors, code


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=ROOT / 'work/program-status-performance' /
                        (time.strftime('%Y%m%dT%H%M%SZ', time.gmtime()) + '-' + uuid.uuid4().hex[:8]))
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    before = identity()
    command = ['cargo', 'run', '--locked', '--release', '-p', 'pty-runtime-domain',
               '--example', 'program_status_performance']
    started = time.monotonic()
    output, errors, code = run_benchmark(command)
    (args.output / 'stdout.jsonl').write_text(output)
    (args.output / 'stderr.txt').write_text(errors)
    after = identity()
    metadata = dict(command=command, revision=subprocess.check_output(
        ['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        source_sha256_before=before, source_sha256_after=after,
        sources_unchanged=before == after, platform=platform.platform(), machine=platform.machine(),
        rustc=subprocess.check_output(['rustc', '--version'], text=True).strip(),
        elapsed_seconds=time.monotonic() - started, exit_code=code,
        scope='Five release-build domain trials per workload; excludes PTY, native parser and OS memory.')
    (args.output / 'metadata.json').write_text(json.dumps(metadata, indent=2) + '\n')
    print(f'Program-status performance evidence: {args.output}', flush=True)
    if code:
        print(errors)
        raise SystemExit(code)
    records = [json.loads(line) for line in output.splitlines()]
    names = ['ordinary', 'oversized', 'queries', 'saturated', 'one_byte']
    if [(row['workload'], row['trial']) for row in records] != [(n, t) for n in names for t in range(5)]:
        raise SystemExit('Missing expected workload trials')
    if any(row['bytes'] <= 0 or row['elapsed_ns'] <= 0 for row in records):
        raise SystemExit('Invalid measurement')
    if before != after:
        raise SystemExit('Sources changed while measuring; rerun after freeze')
    print(output, end='')


if __name__ == '__main__':
    main()
