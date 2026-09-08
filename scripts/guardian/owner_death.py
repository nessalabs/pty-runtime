#!/usr/bin/env python3
"""Kill the actual public Runtime owner; prove same-session cleanup and FD closure.

The deliberate setsid child must survive: it documents the cleanup scope boundary.
All descendant cleanup uses private sockets, never reusable numeric PID signals.
"""
import argparse
import errno
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import select
import signal
import socket
import subprocess
import sys
import tempfile
import time


def connect(path, role, **extra):
    channel = socket.socket(socket.AF_UNIX)
    channel.connect(path)
    record = dict(role=role, pid=os.getpid(), sid=os.getsid(0), pgid=os.getpgrp(), **extra)
    channel.sendall(json.dumps(record).encode() + b"\n")
    return channel


def serve(channel):
    while True:
        command = channel.recv(1)
        if command in (b"", b"q"):
            os._exit(0)
        if command == b"p":
            channel.sendall(b"p")


def workload(path, high_fd):
    for signum in (signal.SIGHUP, signal.SIGTERM, signal.SIGTTOU):
        signal.signal(signum, signal.SIG_IGN)
    try:
        os.fstat(high_fd)
        fd_closed = False
    except OSError as error:
        fd_closed = error.errno == errno.EBADF
    sentinel, guardian = os.getsid(0), os.getppid()
    children = []
    for role in ("foreground", "background", "sentinel_member", "guardian_member", "escaped"):
        read_fd, write_fd = os.pipe()
        child = os.fork()
        if child == 0:
            os.close(read_fd)
            if role == "escaped":
                os.setsid()
            else:
                group = sentinel if role == "sentinel_member" else guardian if role == "guardian_member" else 0
                os.setpgid(0, group)
            channel = connect(path, role)
            os.write(write_fd, b"r")
            os.close(write_fd)
            serve(channel)
        os.close(write_fd)
        assert os.read(read_fd, 1) == b"r", role
        os.close(read_fd)
        children.append(child)
    os.tcsetpgrp(0, children[0])
    channel = connect(path, "workload", sentinel=sentinel, guardian=guardian,
                      fd_closed=fd_closed, tty=all(os.isatty(fd) for fd in range(3)))
    serve(channel)


def live_session(sid):
    listing = subprocess.check_output(["ps", "-axo", "pid=,stat="], text=True)
    members = []
    for line in listing.splitlines():
        pid, state = line.split()[:2]
        if state.startswith(("Z", "E")):
            continue
        try:
            if os.getsid(int(pid)) == sid:
                members.append(int(pid))
        except ProcessLookupError:
            pass
    return members


def read_record(channel, deadline):
    data = b""
    while b"\n" not in data:
        channel.settimeout(max(0.01, deadline - time.monotonic()))
        piece = channel.recv(4096)
        assert piece, "member disconnected before readiness"
        data += piece
    return json.loads(data)


def run(binary):
    channels, records = {}, {}
    owner = outside = None
    read_fd = high_fd = None
    with tempfile.TemporaryDirectory(prefix="pty-owner-") as folder:
        path = str(Path(folder) / "control.sock")
        listener = socket.socket(socket.AF_UNIX)
        listener.bind(path)
        listener.listen(8)
        try:
            outside = subprocess.Popen(["/bin/sleep", "300"], start_new_session=True)
            read_fd, write_fd = os.pipe()
            high_fd = fcntl.fcntl(write_fd, fcntl.F_DUPFD, 300)
            os.close(write_fd)
            os.set_inheritable(high_fd, True)
            owner = subprocess.Popen([str(binary), sys.executable, str(Path(__file__).resolve()), path,
                                      str(high_fd)], pass_fds=(high_fd,), stdin=subprocess.PIPE,
                                     stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            os.close(high_fd)
            high_fd = None
            deadline = time.monotonic() + 20
            while len(records) < 6:
                listener.settimeout(max(0.01, deadline - time.monotonic()))
                channel, _ = listener.accept()
                record = read_record(channel, deadline)
                channels[record["role"]] = channel
                records[record["role"]] = record
            assert select.select([owner.stdout], [], [], 5)[0], "owner readiness timeout"
            ready = owner.stdout.readline().decode().strip()
            work = records["workload"]
            assert ready == f'OWNER_READY {work["pid"]}', ready
            assert work["fd_closed"] and work["tty"], work
            assert select.select([read_fd], [], [], 5)[0], "helper inherited high non-CLOEXEC writer"
            assert os.read(read_fd, 1) == b"", "unexpected probe pipe data"
            sid = work["sid"]
            assert len({work["pid"], work["guardian"], sid}) == 3
            for role in ("foreground", "background", "sentinel_member", "guardian_member"):
                assert records[role]["sid"] == sid, records[role]
            assert records["sentinel_member"]["pgid"] == sid
            assert records["guardian_member"]["pgid"] == work["guardian"]
            assert records["escaped"]["sid"] != sid
            assert os.getsid(outside.pid) != sid
            started = time.monotonic()
            owner.kill()  # Exact unreaped direct child, whose identity cannot be reused.
            assert owner.wait(timeout=5) == -signal.SIGKILL
            remaining = live_session(sid)
            while remaining and time.monotonic() - started < 10:
                time.sleep(0.025)
                remaining = live_session(sid)
            assert not remaining, f"same-session survivors: {remaining}"
            assert outside.poll() is None, "outside-session control died"
            escaped = channels["escaped"]
            escaped.settimeout(2)
            escaped.sendall(b"p")
            assert escaped.recv(1) == b"p", "deliberate setsid escape did not survive"
            return dict(passed=True, platform=platform.platform(), records=records,
                        cleanup_seconds=time.monotonic() - started, high_non_cloexec_fd_closed=True,
                        outside_survived=True, deliberate_setsid_escape_survived=True,
                        owner_exit=owner.returncode, binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                        driver_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest())
        finally:
            for channel in channels.values():
                try:
                    channel.sendall(b"q")
                except OSError:
                    pass
                channel.close()
            if owner is not None and owner.poll() is None:
                owner.kill()
                owner.wait(timeout=5)
            if outside is not None and outside.poll() is None:
                outside.kill()
                outside.wait(timeout=5)
            for descriptor in (read_fd, high_fd):
                if descriptor is not None:
                    os.close(descriptor)
            listener.close()


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "--workload":
        workload(sys.argv[2], int(sys.argv[3]))
        return
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--iterations", type=int, default=1)
    args = parser.parse_args()
    for iteration in range(args.iterations):
        result = run(args.binary.resolve())
        print(json.dumps(dict(iteration=iteration + 1, **result)), flush=True)


if __name__ == "__main__":
    main()
