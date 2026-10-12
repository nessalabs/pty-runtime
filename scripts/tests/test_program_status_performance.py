"""Real timeout evidence for the benchmark's Cargo/child lifetime boundary."""
import os
import json
from pathlib import Path
import subprocess
import signal
import socket
import sys
import time
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from program_status_performance import run_benchmark


@unittest.skipUnless(os.name == 'posix', 'process-group contract is Unix-only')
class ProgramStatusPerformanceTests(unittest.TestCase):
    def fake_child(self):
        child = mock.Mock()
        child.pid = 12345
        child.returncode = None
        child.wait.return_value = 0
        return child

    def test_interrupt_after_timeout_closed_pipes_does_not_reenter_communicate(self):
        child = self.fake_child()
        interruption = KeyboardInterrupt('original')
        child.communicate.side_effect = [subprocess.TimeoutExpired('fixture', 1),
            subprocess.TimeoutExpired('fixture', 10), ValueError('closed pipe')]
        child.wait.side_effect = [interruption, 0]
        with mock.patch('program_status_performance.subprocess.Popen', return_value=child), \
             mock.patch('program_status_performance.os.killpg'):
            with self.assertRaises(KeyboardInterrupt) as raised:
                run_benchmark(['fixture'])
        self.assertIs(raised.exception, interruption)
        self.assertEqual(child.communicate.call_count, 2)
        self.assertEqual(child.wait.call_count, 2)
        self.assertTrue(child.stdout.close.called)
        self.assertTrue(child.stderr.close.called)

    def test_repeated_interrupts_retry_cleanup_with_one_deadline(self):
        child = self.fake_child()
        original = KeyboardInterrupt('original')
        child.communicate.side_effect = [original, ValueError('unexpected communicate re-entry')]
        child.wait.side_effect = [KeyboardInterrupt('repeat'), 0]
        ticks = iter(range(100, 120))
        with mock.patch('program_status_performance.subprocess.Popen', return_value=child), \
             mock.patch('program_status_performance.os.killpg',
                        side_effect=[KeyboardInterrupt('repeat'), None]) as stop, \
             mock.patch('program_status_performance.time.monotonic', side_effect=lambda: next(ticks)):
            with self.assertRaises(KeyboardInterrupt) as raised:
                run_benchmark(['fixture'])
        self.assertIs(raised.exception, original)
        self.assertEqual(stop.call_count, 2)
        self.assertEqual(child.communicate.call_count, 1)
        timeouts = [call.kwargs['timeout'] for call in child.wait.call_args_list]
        self.assertEqual(len(timeouts), 2)
        self.assertGreater(timeouts[0], timeouts[1])
        self.assertGreater(timeouts[1], 0)
        self.assertLessEqual(timeouts[0], 10)
        child.stdout.close.assert_called_once()
        child.stderr.close.assert_called_once()

    def test_reaped_leader_is_not_used_for_a_group_signal(self):
        child = self.fake_child()
        child.returncode = 0
        original = KeyboardInterrupt('original')
        child.communicate.side_effect = [original, ValueError('unexpected communicate re-entry')]
        with mock.patch('program_status_performance.subprocess.Popen', return_value=child), \
             mock.patch('program_status_performance.os.killpg') as stop:
            with self.assertRaises(KeyboardInterrupt) as raised:
                run_benchmark(['fixture'])
        self.assertIs(raised.exception, original)
        stop.assert_not_called()
        child.stdout.close.assert_called_once()
        child.stderr.close.assert_called_once()
        child.wait.assert_called_once()

    def test_cleanup_failure_preserves_original_interrupt_on_supported_python(self):
        child = self.fake_child()
        original = KeyboardInterrupt('original')
        child.communicate.side_effect = [original, ('', '')]
        child.wait.side_effect = subprocess.TimeoutExpired('fixture', 0)
        with mock.patch('program_status_performance.subprocess.Popen', return_value=child), \
             mock.patch('program_status_performance.os.killpg', side_effect=OSError('refused')):
            with self.assertRaises(KeyboardInterrupt) as raised:
                run_benchmark(['fixture'])
        self.assertIs(raised.exception, original)
        self.assertIsInstance(raised.exception.cleanup_failures, tuple)
        self.assertEqual(len(raised.exception.cleanup_failures), 2)
        child.stdout.close.assert_called_once()
        child.stderr.close.assert_called_once()

    def test_interrupt_stops_the_live_launcher_and_descendant_and_propagates(self):
        fixture = '''import json,os,signal,socket,subprocess,sys
channel=socket.socket(socket.AF_UNIX)
try:
 channel.connect(sys.argv[1])
 child=subprocess.Popen([sys.executable,"-c","import time; time.sleep(60)"])
 channel.sendall((json.dumps([os.getpid(),child.pid])+"\\n").encode())
 channel.recv(1)
finally:
 os.kill(0,signal.SIGKILL)
'''
        runner = '''import sys
sys.path.insert(0,sys.argv[1])
from program_status_performance import run_benchmark
try:
 run_benchmark([sys.executable,"-c",sys.argv[2],sys.argv[3]])
except KeyboardInterrupt:
 print("interrupted",flush=True)
 sys.exit(130)
raise SystemExit("interrupt was swallowed")
'''
        with tempfile.TemporaryDirectory() as folder:
            address = str(Path(folder) / 'release.sock')
            with socket.socket(socket.AF_UNIX) as listener:
                listener.bind(address)
                listener.listen(1)
                listener.settimeout(5)
                child = subprocess.Popen([sys.executable, '-c', runner,
                    str(Path(__file__).resolve().parents[1]), fixture, address],
                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                    start_new_session=True)
                connection = None
                try:
                    connection, _ = listener.accept()
                    connection.settimeout(5)
                    with connection.makefile('rb') as ready:
                        launcher, descendant = json.loads(ready.readline())
                    self.assertEqual(os.getpgid(launcher), launcher)
                    self.assertEqual(os.getpgid(descendant), launcher)
                    os.kill(child.pid, signal.SIGINT)
                    output, errors = child.communicate(timeout=5)
                    self.assertEqual(child.returncode, 130, errors)
                    self.assertEqual(output, 'interrupted\n')
                    for pid in (launcher, descendant):
                        status = subprocess.run(['ps', '-p', str(pid), '-o', 'stat='],
                            capture_output=True, text=True, check=False).stdout.strip()
                        self.assertTrue(not status or status.startswith('Z'),
                                        f'interrupt left process {pid} alive: {status}')
                finally:
                    # EOF tells the still-live fixture to kill its actual own
                    # group; never signal a cached descendant or group number.
                    if connection is not None:
                        connection.close()
                    if child.poll() is None:
                        child.kill()
                        child.communicate(timeout=5)

    def test_timeout_stops_benchmark_descendant_as_well_as_launcher(self):
        command = [sys.executable, '-c',
                   'import subprocess,sys,time; '
                   'child=subprocess.Popen([sys.executable,"-c","import time; time.sleep(10)"]); '
                   'print(child.pid,flush=True); time.sleep(60)']
        started = time.monotonic()
        output, _, code = run_benchmark(command, timeout_seconds=1)
        self.assertLess(time.monotonic() - started, 5, "timeout must not wait for descendant exit")
        self.assertEqual(code, 124)
        child = int(output.strip())
        status = subprocess.run(['ps', '-p', str(child), '-o', 'stat='],
                                capture_output=True, text=True, check=False).stdout.strip()
        self.assertTrue(not status or status.startswith('Z'), status)

    def test_success_preserves_stdout_stderr_and_exit_status(self):
        output, errors, code = run_benchmark([
            sys.executable, '-c', 'import sys; print("ok"); print("diagnostic",file=sys.stderr)'])
        self.assertEqual((output, errors, code), ('ok\n', 'diagnostic\n', 0))
