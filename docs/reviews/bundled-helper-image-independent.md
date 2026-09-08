# Independent bundled-helper image acceptance tests

P2 confirmed and independently re-reviewed as resolved: opening a bundled FIFO
waited for a writer before reaching the intended regular-file rejection. The
public `with_bundled_guardian` constructor was exercised in an isolated subprocess
with a fresh private FIFO, no writer, a startup marker and a one-second constructor
observation window. Before the fix, the parent killed and reaped the blocked child
before failing its assertion. Other file-type tests passed. Original source/hash
and red log are retained under `docs/verification/bundled-helper-image`.

This is an avoidable wait on an unsupported local file type, not a newly promised
wall-clock deadline for arbitrary filesystem calls. The backend documentation's
qualification about uninterruptible OS/filesystem stalls remains applicable;
regular/network file access is not made cancellable by this change.

Root added O_NONBLOCK only to the bundled source's read-only O_NOFOLLOW/O_CLOEXEC
open. The existing descriptor metadata check at image.rs:28 still admits only
regular files; FIFO now reaches InvalidCommand without waiting for a peer. The
bounded IMAGE.len()+1 read, exact byte comparison and private executable staging
are unchanged. No follow-symlink behavior or byte-validation relaxation was added.

The new public acceptance tests also establish:

- Exact embedded bytes are accepted and staged before return: deleting the bundled
  source after construction still permits a real helper-backed PTY command to
  run and return its expected output/exit.
- One altered byte and one trailing byte each produce Unsupported.
- A symlink is rejected, and a directory produces InvalidCommand.
- A FIFO without a peer is rejected as InvalidCommand within the bounded test;
  the separate probe is always killed/reaped on failure.

Tests use only a fresh private directory and child-scoped Command::env; no global
process environment is mutated. There were no existing bundled-image tests in
the inspected integration/unit tree. No production code was changed by this
reviewer; the production flag correction was root-owned after retained red proof.

Independent green command:
`cargo test --locked -p pty-runtime-infrastructure --no-default-features
--test process_bundled_image -- --test-threads=1`: five tests pass in 0.14 seconds
(including the filtered subprocess probe entry). Focused no-default Clippy and
rustfmt checks pass. These bounded macOS checks ran concurrently with root's local
capacity workload; no Linux workload, native corpus or full gate was run.

No remaining P1/P2 in this source/type-validation scope. Signed distribution,
target-build packaging and arbitrary filesystem availability remain separate
qualification obligations. Evidence contains before/after source identities,
red/green logs and the focused Clippy log; root owns the final candidate gate.
