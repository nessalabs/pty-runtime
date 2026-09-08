#!/usr/bin/env python3
"""Exercise the real interactive example through an outer PTY, including restoration."""
import argparse
import errno
import fcntl
import json
import os
from pathlib import Path
import select
import signal
import struct
import subprocess
import sys
import termios
import platform
import time

FIXTURE = r'''
import os, sys, signal
if len(sys.argv) > 1 and sys.argv[1] == 'suspend':
    signal.signal(signal.SIGTSTP, lambda signum, frame: print('CHILD-TSTP', flush=True))
print("READY:" + str(os.getpid()), flush=True)
print("UTF8:界 e\u0301 😀\x1b[31mred\x1b[0m", flush=True)
if len(sys.argv) > 1 and sys.argv[1] == 'stall':
    import tty, time
    tty.setraw(0)
    print('STALLED', flush=True)
    time.sleep(60)
for line in sys.stdin:
    if line.strip() == "size":
        value = os.get_terminal_size()
        print("SIZE:%d:%d" % (value.columns, value.lines), flush=True)
    elif line.strip() == "quit":
        sys.exit(7)
    else:
        print("ECHO:" + line.strip(), flush=True)
'''


def run(binary, action):
    started = time.monotonic()
    cancel_seconds = None
    master, slave = os.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 24, 80, 0, 0))
    attributes = termios.tcgetattr(slave)
    command = [str(binary), sys.executable, '-u', '-c', FIXTURE]
    if action == 'stalled-input-cancel':
        command.append('stall')
    elif action == 'child-suspend':
        command.append('suspend')
    if action == 'spawn-failure':
        command = [str(binary), '/pty-interactive-intentionally-missing']
    def controlling_terminal():
        # This single-threaded test child becomes the foreground owner, as a
        # job-control shell would arrange for an interactive client.
        fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
        os.tcsetpgrp(slave, os.getpgrp())
    report_read, report_write = os.pipe()
    process = subprocess.Popen([sys.executable, str(Path(__file__).resolve()), '--host',
                                str(report_write), *command],
                               stdin=slave, stdout=slave, stderr=slave,
                               pass_fds=(report_write,), start_new_session=True,
                               preexec_fn=controlling_terminal)
    os.close(report_write)
    transcript = bytearray()
    workload = None
    def receive(marker):
        deadline = time.monotonic()+15
        while marker not in transcript:
            assert time.monotonic() < deadline, (action, marker, transcript[-500:])
            readable, _, _ = select.select([master], [], [], 0.1)
            if readable:
                try:
                    data = os.read(master, 65536)
                except OSError as error:
                    if error.errno != errno.EIO:
                        raise
                    data = b''
                assert data, ('early EOF', action, transcript[-500:])
                transcript.extend(data)
    def wait_child(timeout):
        deadline = time.monotonic() + timeout
        while process.poll() is None:
            assert time.monotonic() < deadline, ('exit timeout', action, transcript[-1000:])
            if select.select([master], [], [], 0.02)[0]:
                try:
                    transcript.extend(os.read(master, 65536))
                except OSError as error:
                    if error.errno != errno.EIO:
                        raise
        return process.wait()
    try:
        if action != 'spawn-failure':
            receive(b'UTF8:')
            line = bytes(transcript).split(b'READY:')[1].split(b'\r')[0].split(b'\n')[0]
            workload = int(line)
            assert termios.tcgetattr(slave) != attributes, 'host was not placed in raw mode'
            if action == 'input-resize-exit':
                os.write(master, b'hello-world\r')
                receive(b'ECHO:hello-world')
                fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 40, 120, 0, 0))
                time.sleep(0.15)
                os.write(master, b'size\r')
                receive(b'SIZE:120:40')
                assert '界 e\u0301 😀'.encode() in transcript
                assert b'\x1b[31mred\x1b[0m' in transcript
                os.write(master, b'quit\r')
            elif action == 'stalled-input-cancel':
                receive(b'STALLED')
                # Fill the inner raw input queue while leaving room in the outer
                # queue for the signal character. The child never reads input.
                amount = 2000 if platform.system() == 'Darwin' else 6000
                for _ in range(amount // 250):
                    assert os.write(master, b'x' * 250) == 250
                    time.sleep(0.03)
                cancelled_at = time.monotonic()
                assert os.write(master, b'\x1d') == 1
                assert wait_child(2) == 130, 'emergency cancel did not return SIGINT status'
                cancel_seconds = time.monotonic() - cancelled_at
                assert cancel_seconds < 2
            elif action == 'escape-cancel':
                os.write(master, b'\x1d')
            elif action == 'external-term':
                process.send_signal(signal.SIGTERM)
            elif action == 'child-suspend':
                os.write(master, b'\x1a')
                receive(b'CHILD-TSTP')
                os.write(master, b'quit\r')
            elif action == 'child-interrupt':
                os.write(master, b'\x03')
        code = wait_child(15)
        expected_code = {'input-resize-exit': 7, 'child-suspend': 7, 'external-term': 143,
                         'spawn-failure': 1}.get(action, 130)
        assert code == expected_code, (action, code)
        report = json.loads(os.read(report_read, 4096))
        assert report['termios_restored'], ('termios not restored', action)
        assert report['flags_restored'], ('descriptor flags not restored', action)
        if workload is not None:
            exists = subprocess.run(['ps', '-p', str(workload), '-o', 'pid='], capture_output=True).stdout.strip()
            assert not exists, ('workload retained', workload)
        return dict(action=action, passed=True, exit_code=code, captured_bytes=len(transcript),
                    termios_restored=True, descriptor_flags_restored=True, workload_reaped=True,
                    elapsed_seconds=time.monotonic() - started, cancel_seconds=cancel_seconds)
    finally:
        if process.poll() is None:
            process.kill()
        os.close(report_read)
        os.close(master)
        os.close(slave)
        process.wait(timeout=15)


def host(report_fd, command):
    # Keep the controlling session leader alive through restoration inspection:
    # Darwin revokes the slave when that leader exits. Only the client changes
    # the terminal. The wrapper ignores group SIGINT and forwards external TERM.
    attributes = termios.tcgetattr(0)
    flags = fcntl.fcntl(0, fcntl.F_GETFL)
    signal.signal(signal.SIGINT, lambda signum, frame: None)
    child = subprocess.Popen(command)
    def terminate(signum, frame):
        if child.poll() is None:
            child.send_signal(signum)
    signal.signal(signal.SIGTERM, terminate)
    code = child.wait()
    restored = termios.tcgetattr(0)
    restored[3] &= ~getattr(termios, 'PENDIN', 0)
    attributes[3] &= ~getattr(termios, 'PENDIN', 0)
    mask = os.O_NONBLOCK | os.O_APPEND | os.O_ASYNC | os.O_ACCMODE
    report = dict(termios_restored=restored == attributes,
                  flags_restored=fcntl.fcntl(0, fcntl.F_GETFL) & mask == flags & mask)
    os.write(report_fd, json.dumps(report).encode())
    os.close(report_fd)
    sys.exit(code if code >= 0 else 128 - code)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=Path('target/debug/examples/interactive'))
    args = parser.parse_args()
    for action in ('input-resize-exit', 'escape-cancel', 'external-term', 'child-interrupt', 'child-suspend', 'stalled-input-cancel', 'spawn-failure'):
        print(json.dumps(run(args.binary.resolve(), action)), flush=True)


if __name__ == '__main__':
    if len(sys.argv) > 1 and sys.argv[1] == '--host':
        host(int(sys.argv[2]), sys.argv[3:])
    else:
        main()
