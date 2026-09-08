#!/usr/bin/env python3
"""Exercise the actual packaged helper with independent PTY/control endpoints."""
import argparse
import fcntl
import json
import os
from pathlib import Path
import select
import signal
import socket
import struct
import subprocess
import termios
import time

FRAME = struct.Struct('<4sHHQiiii')


class Session:
    def __init__(self, executable, command, grace=40, arguments=None):
        self.generation = time.monotonic_ns() & ((1 << 63) - 1)
        self.master, slave = os.openpty()
        attrs = termios.tcgetattr(slave)
        attrs[3] &= ~termios.ECHO
        termios.tcsetattr(slave, termios.TCSANOW, attrs)
        self.channels = []
        children = []
        for _ in range(2):
            parent, child = socket.socketpair()
            self.channels.append(parent)
            children.append(child)
        sources = [children[0].fileno(), children[1].fileno(), self.master, slave]
        copies = [fcntl.fcntl(fd, fcntl.F_DUPFD_CLOEXEC, 300) for fd in sources]

        def setup():
            for target, source in enumerate(copies, 3):
                os.dup2(source, target)

        command_arguments = ['/bin/sh', '-c', command] if arguments is None else arguments
        self.process = subprocess.Popen(
            [str(executable), '--pty-runtime-guardian-v1', str(self.generation),
             str(grace), *command_arguments],
            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL, close_fds=False, preexec_fn=setup, env={})
        for fd in copies:
            os.close(fd)
        for child in children:
            child.close()
        os.close(slave)
        os.set_blocking(self.master, False)
        for channel in self.channels:
            channel.setblocking(False)
        self.pending = [bytearray(), bytearray()]
        self.closed = [False, False]
        self.frames = []
        self.output = bytearray()
        self.ids = None

    def pump(self, timeout=0.02):
        readable = [self.master]
        readable += [channel for index, channel in enumerate(self.channels) if not self.closed[index]]
        ready, _, _ = select.select(readable, [], [], timeout)
        for descriptor in ready:
            if descriptor == self.master:
                try:
                    self.output.extend(os.read(self.master, 65536))
                except OSError:
                    pass
                continue
            index = self.channels.index(descriptor)
            data = descriptor.recv(4096)
            if not data:
                self.closed[index] = True
                continue
            self.pending[index].extend(data)
            while len(self.pending[index]) >= FRAME.size:
                packet = bytes(self.pending[index][:FRAME.size])
                del self.pending[index][:FRAME.size]
                magic, version, kind, generation, *values = FRAME.unpack(packet)
                assert (magic, version, generation) == (b'PTGR', 1, self.generation)
                self.frames.append((index, kind, values))
                if kind == 2:
                    self.ids = values

    def wait(self, condition, timeout=5):
        deadline = time.monotonic() + timeout
        while not condition():
            if time.monotonic() >= deadline:
                raise TimeoutError({'frames': self.frames, 'closed': self.closed,
                                    'output': bytes(self.output[-200:]), 'sentinel': self.process.pid})
            self.pump()

    def admitted(self):
        self.wait(lambda: {index for index, kind, _ in self.frames if kind == 1} == {0, 1})
        self.send(16)
        self.wait(lambda: self.ids is not None and any(kind == 3 for _, kind, _ in self.frames))
        return self.ids

    def send(self, kind):
        packet = FRAME.pack(b'PTGR', 1, kind, self.generation, 0, 0, 0, 0)
        for channel in self.channels:
            try:
                channel.sendall(packet)
            except OSError:
                pass

    def finished(self):
        self.wait(lambda: all(self.closed) and self.process.poll() is not None)

    def close(self):
        self.send(14)
        for channel in self.channels:
            channel.close()
        self.closed = [True, True]
        self.wait(lambda: self.process.poll() is not None)
        os.close(self.master)


def smoke(executable):
    session = Session(executable, "printf ready; read x; printf accepted; exit 7")
    try:
        workload, guardian, sentinel, sid = session.admitted()
        assert len({workload, guardian, sentinel}) == 3 and sentinel == sid
        session.wait(lambda: session.output == b'ready')
        os.write(session.master, b'go\n')
        session.wait(lambda: any(kind == 4 for _, kind, _ in session.frames))
        exits = [values[0] for _, kind, values in session.frames if kind == 4]
        assert all(os.waitstatus_to_exitcode(status) == 7 for status in exits)
        session.send(12)
        session.finished()
        assert session.output == b'readyaccepted'
        assert not any(kind == 6 for _, kind, _ in session.frames), session.frames
        return {'case': 'actual_exit_literal_input', 'result': 'pass', 'actual_exit': 7}
    finally:
        session.close()


def failure(executable, mode):
    command = "exec /bin/sh -i -c 'set -m; trap \"\" HUP TERM; sleep 300 & printf ready; sleep 300'"
    session = Session(executable, command)
    try:
        workload, guardian, sentinel, sid = session.admitted()
        session.wait(lambda: b'ready' in session.output and os.tcgetpgrp(session.master) != workload)
        target = sentinel if mode == 'sentinel_kill' else guardian
        os.kill(target, signal.SIGABRT if mode == 'guardian_abort' else signal.SIGKILL)
        session.finished()
        assert not live_in_session(sid)
        assert any(kind == 6 for _, kind, _ in session.frames)
        if mode != 'sentinel_kill':
            assert not any(kind == 4 for _, kind, _ in session.frames)
        return {'case': mode, 'result': 'pass', 'sid': sid, 'workload': workload,
                'actual_exit_observed': any(kind == 4 for _, kind, _ in session.frames)}
    finally:
        session.close()


def reserved_groups(executable, mode, damage=None):
    fixture = executable.parent / 'guardian-fixture'
    session = Session(executable, '', arguments=[str(fixture), mode])
    try:
        workload, guardian, sentinel, sid = session.admitted()
        session.wait(lambda: b'\n' in session.output)
        entrants = [int(pid) for pid in session.output.split()[1:]]
        if damage:
            os.kill(sentinel if damage == 'sentinel' else guardian, signal.SIGKILL)
        else:
            os.write(session.master, b'finish\n')
            session.wait(lambda: any(kind == 4 for _, kind, _ in session.frames))
            session.send(12)
        session.finished()
        assert not live_in_session(sid)
        for pid in entrants:
            try:
                os.getsid(pid)
            except ProcessLookupError:
                continue
            # Linux's init can reap orphan zombies asynchronously; they are dead.
            stat = Path(f'/proc/{pid}/stat')
            assert stat.exists() and stat.read_text().rsplit(')', 1)[1].split()[0] == 'Z', pid
        assert any(kind == 6 for _, kind, _ in session.frames) == bool(damage), session.frames
        return {'case': f'helper_group_{mode}_{damage or "normal"}', 'result': 'pass',
                'successor_admitted': any(kind == 15 for _, kind, _ in session.frames)}
    finally:
        session.close()


def live_in_session(sid):
    live = []
    snapshot = subprocess.check_output(['ps', '-axo', 'pid=,stat='], text=True)
    for line in snapshot.splitlines():
        pid, state = line.split()
        if 'Z' in state or 'E' in state:
            continue
        try:
            if os.getsid(int(pid)) == sid:
                live.append(int(pid))
        except ProcessLookupError:
            pass
    return live


def handoff_failure(executable, target):
    fixture = executable.parent / 'guardian-fixture'
    session = Session(executable, '', arguments=[str(fixture), 'both'])
    sentinel = None
    try:
        _, guardian, sentinel, sid = session.admitted()
        session.wait(lambda: b'\n' in session.output)
        os.kill(sentinel, signal.SIGSTOP)
        os.write(session.master, b'finish\n')
        session.wait(lambda: any(kind == 4 for _, kind, _ in session.frames))
        session.send(12)
        session.wait(lambda: any(kind == 15 for _, kind, _ in session.frames))
        successor = next(values[0] for _, kind, values in session.frames if kind == 15)
        victim = {'successor': successor, 'guardian': guardian, 'sentinel': sentinel}[target]
        os.kill(victim, signal.SIGKILL)
        if victim != sentinel:
            os.kill(sentinel, signal.SIGCONT)
        session.finished()
        assert not live_in_session(sid)
        return {'case': f'handoff_{target}_kill_before_ack', 'result': 'pass'}
    finally:
        if sentinel is not None:
            try:
                os.kill(sentinel, signal.SIGCONT)
            except ProcessLookupError:
                pass
        session.close()


def owner_eof(executable, before_exec):
    session = Session(executable, 'printf ready; sleep 300')
    try:
        if before_exec:
            session.wait(lambda: {index for index, kind, _ in session.frames if kind == 1} == {0, 1})
            sid = session.process.pid
        else:
            _, _, _, sid = session.admitted()
            session.wait(lambda: session.output == b'ready')
        for channel in session.channels:
            channel.close()
        session.closed = [True, True]
        session.wait(lambda: session.process.poll() is not None and not live_in_session(sid))
        if before_exec:
            assert not session.output and session.ids is None
        return {'case': f'owner_eof_{"before" if before_exec else "after"}_exec', 'result': 'pass'}
    finally:
        session.close()


def acknowledged_handoff_failure(executable, target):
    fixture = executable.parent / 'guardian-fixture'
    session = Session(executable, '', arguments=[str(fixture), 'both'])
    stopped = set()
    try:
        _, guardian, sentinel, sid = session.admitted()
        session.wait(lambda: b'\n' in session.output)
        os.kill(sentinel, signal.SIGSTOP)
        stopped.add(sentinel)
        os.write(session.master, b'finish\n')
        session.wait(lambda: any(kind == 4 for _, kind, _ in session.frames))
        session.send(12)
        session.wait(lambda: any(kind == 15 for _, kind, _ in session.frames))
        successor = next(values[0] for _, kind, values in session.frames if kind == 15)
        os.kill(successor, signal.SIGSTOP)
        stopped.add(successor)
        os.kill(sentinel, signal.SIGCONT)
        stopped.remove(sentinel)
        session.wait(lambda: any(index == 0 and kind == 15 for index, kind, _ in session.frames))
        # C is stopped, so S has only its acknowledgement to flush before parking.
        # The assertion below uses G's actual retirement, not this scheduling delay,
        # as evidence that C subsequently consumed the acknowledgement.
        time.sleep(0.05)
        os.kill(sentinel, signal.SIGSTOP)
        stopped.add(sentinel)
        os.kill(successor, signal.SIGCONT)
        stopped.remove(successor)
        session.wait(lambda: guardian not in live_in_session(sid))
        assert successor in live_in_session(sid)
        victim = successor if target == 'successor' else sentinel
        os.kill(victim, signal.SIGKILL)
        stopped.discard(victim)
        if victim != sentinel:
            os.kill(sentinel, signal.SIGCONT)
            stopped.remove(sentinel)
        session.finished()
        assert not live_in_session(sid)
        # A queued Finish may already authorize the surviving group to retire.
        # The host also reports loss for EOF without that endpoint's Retiring,
        # even if the survivor need not send a separate Fault before dying.
        retired = {index for index, kind, _ in session.frames if kind == 7}
        assert any(kind == 6 for _, kind, _ in session.frames) or retired != {0, 1}
        return {'case': f'handoff_{target}_kill_after_ack', 'result': 'pass',
                'old_guardian_retired_before_fault': True}
    finally:
        for pid in stopped:
            try:
                os.kill(pid, signal.SIGCONT)
            except ProcessLookupError:
                pass
        session.close()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--helper', type=Path, required=True)
    args = parser.parse_args()
    outside = subprocess.Popen(['/bin/sleep', '300'], start_new_session=True)
    try:
        print(json.dumps(smoke(args.helper)), flush=True)
        for mode in ['sentinel_kill', 'guardian_kill', 'guardian_abort']:
            print(json.dumps(failure(args.helper, mode)), flush=True)
        for mode in ['sentinel', 'guardian', 'both']:
            for damage in [None, 'sentinel', 'guardian']:
                print(json.dumps(reserved_groups(args.helper, mode, damage)), flush=True)
        for before in [True, False]:
            print(json.dumps(owner_eof(args.helper, before)), flush=True)
        for target in ['successor', 'guardian', 'sentinel']:
            print(json.dumps(handoff_failure(args.helper, target)), flush=True)
        for target in ['successor', 'sentinel']:
            print(json.dumps(acknowledged_handoff_failure(args.helper, target)), flush=True)
        assert outside.poll() is None, 'outside-session control process was affected'
    finally:
        outside.kill()
        outside.wait()


if __name__ == '__main__':
    main()
