#!/usr/bin/env python3
"""Run full five-by-sixty-second release throughput trials; --smoke is not acceptance."""
import argparse
import json
import os
from pathlib import Path
import queue
import shutil
import subprocess
import tempfile
import threading
import time
import traceback
from load_support import census, identity, matrix, reporting

# Acknowledged final process-tree census. Periodic sampling stops afterward so a
# normal owner exit while draining completion output is not a lookup failure.
# Anything forked after this phase and before exit is intentionally unsampled.
FINAL_CENSUS_PHASE = 'closed'


def trial(binary, config, destination, smoke, repeat, metadata, sample_seconds):
    command = [str(binary)]
    for key, value in config.items():
        # The fixture spells every flag with hyphens. Sending an underscore
        # meant it fell back to its default, and a four-point sweep measured
        # one point four times before anyone noticed.
        flag = '--' + key.replace('_', '-')
        if isinstance(value, bool):
            if value:
                command.append(flag)
        else:
            command += [flag, str(value)]
    process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                               stderr=subprocess.STDOUT, text=True, bufsize=1,
                               env={**os.environ, 'PTY_RELEASE_CENSUS': '1'})
    incoming = queue.Queue()
    def read():
        for line in process.stdout:
            incoming.put(line)
        incoming.put(None)
    reader = threading.Thread(target=read, daemon=True)
    reader.start()
    workloads = set()
    checkpoints = {}
    complete = False
    latencies = {}
    starts = []
    targets = []
    started = time.monotonic()
    deadline = started + config['seconds'] + config['warmup'] + 180
    next_sample = started + sample_seconds
    ended = False
    last_event = None
    last_checkpoint = None
    with destination.open('w') as output:
        def record(value):
            output.write(json.dumps(value) + '\n')
            output.flush()
        def close_pipes():
            streams = [('stdin', process.stdin)]
            if not reader.is_alive():
                streams.append(('stdout', process.stdout))
            for name, stream in streams:
                try:
                    stream.close()
                except OSError as cleanup_error:
                    record(dict(event='trial_cleanup_failure', stream=name,
                                error=repr(cleanup_error)))
        record({**metadata, 'command': command, 'configuration': config, 'smoke': smoke,
                'repeat': repeat, 'owner_pid': process.pid, 'sample_seconds': sample_seconds})
        try:
            while not ended:
                if time.monotonic() > deadline:
                    raise TimeoutError('trial deadline exceeded; producer/control/parser did not finish')
                try:
                    line = incoming.get(timeout=0.2)
                except queue.Empty:
                    line = ''
                if line is None:
                    ended = True
                    continue
                if line:
                    try:
                        event = json.loads(line)
                    except json.JSONDecodeError:
                        record(dict(event='stderr', text=line.rstrip()))
                        continue
                    record(event)
                    last_event = event.get('event')
                    if event.get('event') == 'latency':
                        latencies[event['boundary']] = event
                    if event.get('event') == 'producer_start' and event['phase'] == 1:
                        starts.append(event['unix_ns'])
                    if event.get('event') == 'child':
                        workloads.add(event['pid'])
                    if event.get('event') == 'complete':
                        complete = True
                    if event.get('event') == 'checkpoint':
                        phase = event['phase']
                        last_checkpoint = phase
                        snapshot = census.sample(process.pid, workloads, phase)
                        checkpoints[phase] = snapshot
                        record(snapshot)
                        if phase == FINAL_CENSUS_PHASE:
                            assert snapshot['tree_processes'] == 1, snapshot
                            assert not snapshot['zombies'], snapshot
                        process.stdin.write('continue\n')
                        process.stdin.flush()
                # Final census is authoritative; the owner may exit while queued
                # completion output is still being drained.
                if FINAL_CENSUS_PHASE not in checkpoints and time.monotonic() >= next_sample and process.poll() is None:
                    record(census.sample(process.pid, workloads, 'periodic'))
                    next_sample = time.monotonic() + sample_seconds
            result = process.wait(timeout=10)
            reader.join(timeout=2)
            close_pipes()
            assert result == 0 and complete, ('trial failed', result, complete)
            assert FINAL_CENSUS_PHASE in checkpoints, 'trial omitted final closed census'
            if starts:
                record(dict(event='producer_start_skew', nanoseconds=max(starts)-min(starts), count=len(starts)))
            if config['active'] > 0:
                for boundary, ceiling in [('InputDispatch', 20000), ('RawOutput', 20000), ('ProjectedOutput', 20000), ('ResizeDispatch', 100000), ('CancelDispatch', 100000)]:
                    if boundary == 'ProjectedOutput' and config['raw']:
                        continue
                    value = latencies.get(boundary, {})
                    observed = value.get('p99_us')
                    failures, unavailable = value.get('failures', 0), value.get('unavailable', 0)
                    target = dict(event='latency_target', boundary=boundary, target_p99_us=ceiling,
                                  observed_p99_us=observed, failures=failures, unavailable=unavailable,
                                  measurement_complete=observed is not None and unavailable == 0,
                                  passed=reporting.measurement_verdict(observed, ceiling, failures, unavailable))
                    targets.append(target)
                    record(target)
            cpu = census.cpu_delta(checkpoints['measurement_start'], checkpoints['measurement_end'])
            record(cpu)
            if config['mode'] == 'idle':
                owner = cpu['categories']['owner']
                record(dict(event='idle_cpu_target', target_core_percent=1.0,
                            measured_core_percent=owner['core_percent'],
                            measurement_complete=owner['core_percent'] is not None,
                            passed=reporting.measurement_verdict(owner['core_percent'], 1.0),
                            acceptance_duration=config['seconds'] >= 60 and not smoke))
            record(dict(event='trial_result', passed=True, seconds=time.monotonic()-started,
                        full_duration_trial=not smoke and config['seconds'] >= 60,
                        latency_targets_passed=reporting.verdict([target['passed'] for target in targets])))
            return True
        except Exception as error:
            record(dict(event='trial_failure', error=repr(error), traceback=traceback.format_exc(),
                        last_event=last_event, last_checkpoint=last_checkpoint))
            # Popen has not reaped this live owner, so its numeric PID cannot be reused.
            # The production guardian owns descendant cleanup after owner death.
            previous_status = process.poll()
            killed = previous_status is None
            if killed:
                process.kill()
            status = process.wait(timeout=30)
            reader.join(timeout=2)
            while True:
                try:
                    pending = incoming.get_nowait()
                except queue.Empty:
                    break
                if pending:
                    try:
                        record(json.loads(pending))
                    except json.JSONDecodeError:
                        record(dict(event='stderr', text=pending.rstrip()))
            record(dict(event='trial_failure_process', exit_code=status,
                        exit_before_cleanup=previous_status, killed_by_driver=killed,
                        output_reader_finished=not reader.is_alive()))
            close_pipes()
            return False


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=Path('target/release/examples/release_load'))
    parser.add_argument('--output', type=Path)
    parser.add_argument('--case', action='append', help='repeat to select cases; default full matrix')
    parser.add_argument('--smoke', action='store_true')
    parser.add_argument('--repeats', type=int, help='defaults to five full trials or one smoke trial')
    parser.add_argument('--sample-seconds', type=float, default=5)
    parser.add_argument('--seconds', type=int,
                        help='override each case\'s measurement duration. The matrix runs 60 s '
                             'because that is what the latency targets are defined over; a longer '
                             'run answers a different question, whether anything accumulates, and '
                             'is not a substitute for the matrix')
    parser.add_argument('--staging-slots', type=int,
                        help='override each projected session\'s parser-output queue depth; '
                             'the fixture default is 256. Recorded in the trial configuration, '
                             'so a sweep is distinguishable from the matrix it is compared against')
    parser.add_argument('--list', action='store_true')
    args = parser.parse_args()
    cases = matrix.cases(args.smoke)
    # What the matrix is, before any override. `full_matrix_executed` is a claim
    # about this, not about having run every case name five times: `--seconds 1`
    # does that and is not the matrix.
    defined = matrix.cases(args.smoke)
    if args.seconds is not None:
        if args.seconds < 1:
            parser.error('a measurement duration must be at least one second')
        for case in cases.values():
            case['seconds'] = args.seconds
    if args.staging_slots is not None:
        if not 1 <= args.staging_slots <= 1_048_576:
            parser.error('staging slots must be within the range the fixture validates')
        for case in cases.values():
            case['staging_slots'] = args.staging_slots
    default = matrix.default_selection(args.smoke)
    if args.list:
        print(json.dumps({'default_selection': default,
                          'host_dependent': list(matrix.HOST_DEPENDENT),
                          'cases': cases}, indent=2))
        return
    if args.output is None:
        parser.error('--output is required for an executed run')
    # The host-dependent cases are selected only when asked for by name: one
    # needs 1,501 processes, and the other cannot pass anywhere at the shipped
    # 1 GiB projection quota, so running them by default would make every
    # default run exit non-zero regardless of what it measured.
    selected = args.case or default
    if any(name not in cases for name in selected):
        parser.error('unknown case; use --list')
    repeats = args.repeats if args.repeats is not None else 1 if args.smoke else 5
    if repeats < 1 or args.sample_seconds <= 0:
        parser.error('repeats and sample interval must be positive')
    args.output.mkdir(parents=True, exist_ok=False)
    metadata = identity.identify(args.binary.resolve())
    with tempfile.TemporaryDirectory(prefix='pty-load-image-') as directory:
        binary = Path(directory) / 'release_load'
        shutil.copy2(args.binary.resolve(), binary)
        results = []
        for name in selected:
            for repeat in range(repeats):
                output = args.output / f'{name}-{repeat+1}.jsonl'
                print(f'{name} trial {repeat+1}/{repeats}', flush=True)
                passed = trial(binary, cases[name], output, args.smoke, repeat+1, metadata, args.sample_seconds)
                results.append(dict(case=name, trial=repeat+1, passed=passed, path=output.name,
                                    **reporting.trial_targets(output, cases[name])))
    summary = dict(smoke=args.smoke, repeats=repeats, results=results,
                   full_matrix_executed=not args.smoke and repeats >= 5
                                        and set(default) <= set(selected)
                                        and all(cases[name] == defined[name] for name in default),
                   all_trials_passed=all(row['passed'] for row in results),
                   all_trials_passed_scope='execution_and_correctness_accounting_only',
                   target_rollup=reporting.rollup(results),
                   limitations=['C/OS allocator overhead is measured by RSS/PSS separately.',
                                'Darwin PSS and portable per-process wakeup counts are unavailable from this collector.',
                                'Rust requested allocation totals include fixture bookkeeping; they are not a standalone 4KiB control-metadata proof.',
                                'Exact raw byte/gap and processed-offset accounting does not replace canonical terminal reference tests.'])
    (args.output / 'summary.json').write_text(json.dumps(summary, indent=2)+'\n')
    if not summary['all_trials_passed']:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
