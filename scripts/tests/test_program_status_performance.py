"""Real timeout evidence for the benchmark's Cargo/child lifetime boundary."""
import os
from pathlib import Path
import subprocess
import sys
import time
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from program_status_performance import run_benchmark


@unittest.skipUnless(os.name == 'posix', 'process-group contract is Unix-only')
class ProgramStatusPerformanceTests(unittest.TestCase):
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
