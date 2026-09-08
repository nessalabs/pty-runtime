# Interactive terminal example

From the repository root, run an interactive shell through the real runtime:

```sh
cargo run --locked --example interactive --no-default-features -- /bin/sh
```

The program defaults to `/bin/sh`. Supply an absolute executable path and literal
arguments to launch another terminal application, for example `/bin/zsh -il`.
The child inherits the environment and current directory. Both stdin and stdout
must be terminals, and the client must own the foreground terminal; this is a local interactive example rather than a pipe filter.
Use `--help` for its short usage message.

The example places the host terminal in raw mode, forwards input to one raw PTY
session, displays original output bytes, and checks host size changes every 50 ms while no earlier resize is pending. Ctrl-C and Ctrl-Z are forwarded as input so
the child's terminal/job-control rules apply. Ctrl-] is reserved as an emergency
cancel key; it terminates the owned session and waits for output drain and cleanup.
Normal child exit returns its exit code. A child signal returns 128 plus the signal.

This example deliberately uses a raw session: the user's outer terminal is the
renderer and answers terminal queries. Attaching a second authoritative terminal
would require deciding which renderer owns replies. It demonstrates actual
process ownership, input acknowledgement, bounded replay, resize and cleanup
without a second terminal model.

Input admission retains at most one 4 KiB chunk at a time. Display retains one
bounded replay page and the session retains at most a 1 MiB replay tail. A display
that falls behind receives an explicit gap; the example reports the missing range
and cleans up instead of displaying an apparently complete terminal stream.
The host terminal generates SIGINT for Ctrl-] independently of queued input, so
emergency cancellation remains responsive when the child stops reading. Other
signal characters are disabled on the host and forwarded to the child; queued
bytes are not flushed by the host signal. Nonblocking host I/O also keeps handled
signals responsive while output is stalled. The runtime owns cancellation and reaping independently.

The terminal guard restores the saved termios and descriptor flags on normal exit,
errors, and handled HUP/TERM/INT/QUIT signals. Abrupt SIGKILL, machine failure, or a
vanished host terminal cannot run restoration; use `stty sane` in the parent shell
if needed. This minimal example exits on externally delivered termination signals;
it does not suspend and resume the outer client itself.

Run the automated smoke against a real outer pseudo-terminal:

```sh
cargo build --locked --example interactive --no-default-features
python3 scripts/release/interactive_smoke.py
```

The smoke checks Unicode/ANSI output, input echo, a real outer-terminal resize
observed inside the child, exit-code propagation, Ctrl-] cancellation, external
TERM, forwarded Ctrl-C/Ctrl-Z, cancellation with a stalled child input queue, failed spawn, restored host terminal configuration and
reaped workload PIDs. Darwin's transient `PENDIN` state and internal descriptor
flags are excluded from comparisons of user-controlled settings. This is an
executed example smoke, not a terminal compatibility or performance qualification.
