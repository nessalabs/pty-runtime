# Experiment 0002: Native terminal compression, parking, and restoration

## Conclusion

The large memory savings come from changing how terminal state is stored and
ensuring freed allocations can leave the process. Reader sharing remains useful,
but [Experiment 0001](0001-pty-speed-and-memory.md) measured a much smaller resource
than filled terminal models.

With 128 native Ghostty terminals containing 10,000 lines each, in-memory history
compression reduced charged process footprint from about **817 MiB to 59 MiB**
for repetitive text, or **338 MiB** for varied text. Complete disk parking with
an experimental reclaimable allocator left about **3.8 MiB** in the process,
including its approximately **1.7 MiB baseline**. The saved snapshots occupied
**96.6 MiB on disk**. These are terminal-model measurements, not a complete
128-session server including PTYs, children, clients, and raw replay.

Restoring usable terminal state can take tens of microseconds when snapshot
bytes are already in memory. Restoring all history takes longer. Keep those two
claims separate. Apply the resulting policy in
[ADR 0003](../adr/0003-session-parking-and-state-transfer.md).

## Method and evidence

Built real `libghostty-vt` from revision
`82232ecde55405559dec29c5466cb9e39938cb41`, using Zig 0.16.0, ReleaseFast, and
the native C API. Scratch C fixtures were compiled with Apple Clang `-O3` and
linked to the static library. No Rust backend implementation was created.

Host: Apple Silicon Mac17,3, arm64, 10 logical CPUs, 24 GiB RAM, macOS 26.6
(25G72), 16 KiB pages. The desktop remained active; this is a local engineering
experiment, with no isolated-core or controlled-storage benchmark environment.

The principal matrix used 1, 32, and 128 terminals, 80 columns by 24 rows, an
8 MiB scrollback limit, disabled image storage, and a 64 KiB parser-continuation
limit. Each terminal received the same 780,000-byte, 10,000-line synthetic corpus
in 16 KiB writes, followed by an unfinished SGR sequence. One corpus repeats
build-log-like text; the other uses deterministic pseudorandom printable text.
Both have 76 characters and CRLF per line. They test different compressibility,
not the distribution of real user workloads.

Each condition ran three times in a fresh process, sequentially. Condition order
was reversed in the second repetition where applicable. The
shared corpus was allocated before baseline measurement. Reported footprint is
`proc_pid_rusage` charged physical footprint; RSS, virtual size, malloc live
bytes, and threads are also recorded. Memory values are stage samples, not
continuous peaks. The 200 ms parked settling interval did not reclaim additional
memory. Tests invoke the parking action directly; they do not measure a 60-second
idle scheduler.

The fixture feeds all terminals, optionally performs incremental compression,
streams each full binary snapshot to an owner-private file, flushes and `fsync`s
it, and then frees all terminal handles. It subsequently restores READY for all
terminals, completes their histories, validates them, and cleans up all snapshots.
Files contain synthetic data only and are not encrypted; encryption cost is
outside these timings.

The recorded set contains 78 lifecycle runs, 9 warmed codec runs with 30 measured
samples each, and a separate correctness fixture. See
[all 1,164 records](data/0002-native-results.jsonl) and
[provenance and source hashes](data/0002-provenance.json).

## Memory at 128 terminals

Values below are medians of three runs, in MiB of **whole fixture process charged
footprint**, including baseline. The first four rows use the default native
allocator. The last row comes from the allocator extension with otherwise
matching terminal data and limits.

| State / strategy | Repetitive text | Varied text |
| --- | ---: | ---: |
| Resident, before compression | 817.09 | 817.06 |
| History compressed in memory | 59.38 | 338.38 |
| Park directly, without prior history compression | 4.91 | 4.94 |
| Compress first, then park using the default allocator | 15.61 | 294.63 |
| Compress first, then park using reclaimable allocations of at least 4 KiB | 3.80 | 3.81 |

Default-allocator compression leaves about **462 KiB of added charged footprint
per terminal** for the repetitive corpus, versus **2.63 MiB** for the varied
corpus. A universal 400 KiB terminal estimate would therefore be misleading.
The empty default-allocator 128-terminal process was about 5.67 MiB, approximately
4.02 MiB above baseline; untouched virtual reservations are separate from that
physical charge.

RSS and footprint behave differently. For example, in a recorded varied-text
run, compression changed RSS from 817.5 to 1,188.8 MiB while charged footprint
fell from 817.1 to 338.4 MiB. The native implementation marks original mappings
reusable through Darwin `MADV_FREE_REUSABLE`; they can remain resident while
becoming reclaimable. The RSS increase does not mean compression increased the
process's non-reclaimable charge, and the footprint reduction does not prove the
machine immediately reused those pages. See the
[pinned reclamation implementation](https://github.com/ghostty-org/ghostty/blob/82232ecde55405559dec29c5466cb9e39938cb41/src/terminal/mem.zig).

### Allocator retention is material

In the 128-terminal varied-text case, freeing all compressed models returned
malloc live bytes to the approximately 0.77 MiB baseline, yet left about 295 MiB
charged to the process. A separate attempt to request maximal malloc pressure
relief returned zero bytes in all 15 diagnostic runs and did not reduce the
sampled parked footprint on this host.

The experimental allocator sends selected allocations to page-rounded anonymous
`mmap` storage and releases them with `munmap`. Smaller allocations use malloc.
A malloc-only callback control and the mapped allocator both decline in-place
resize/remap, so that callback behavior has its own comparison. This prototype
is not a proposed production allocator.

| Compression allocator, 128 terminals | Parked repetitive | Parked varied | Varied compression time per terminal |
| --- | ---: | ---: | ---: |
| Default, matched allocator series | 16.00 MiB | 294.63 MiB | 6.13 ms |
| Malloc callback control | 15.63 MiB | 294.64 MiB | 6.17 ms |
| Map allocations of at least 16 KiB | 15.05 MiB | 4.77 MiB | 6.27 ms |

The 16 KiB threshold misses small, well-compressed buffers. Lowering it to 4 KiB
reduced both parked cases to about 3.8 MiB, but page rounding increased the
repetitive corpus's compressed resident footprint from 59.4 to 82.2 MiB. That
extension ran in a later series; do not attribute small timing differences
between series to its threshold. The result motivates packing small compressed
buffers into shared reclaimable pages, rather than mapping every small buffer.
No such production pool has been built or measured.

Binary snapshot size was exactly 791,636 bytes per 10,000-line terminal in every
strategy and both corpora. Performing in-memory history compression immediately
before parking did not shrink these snapshot files. It added work and could
increase retained allocator memory. Direct parking should not require a prior
compression pass.

## Speed and wake latency

In the original 128-terminal series, history compression took a median **0.66 ms
per repetitive terminal** or **6.18 ms per varied terminal**. The largest observed
incremental steps in those cases were about 87 and 566 microseconds respectively.
Bound and schedule that work; compression is not free merely because it runs
when output is quiet.

Warmed codec tests use already-allocated snapshot bytes, five warm-up iterations,
and 30 measured iterations per fresh process. Values are medians of the three
per-run medians. Encoding reuses an output buffer. Decode times include decoder
creation and terminal construction, but exclude destruction and disk I/O.

| Terminal history | Encode | Restore READY | Restore entire snapshot |
| --- | ---: | ---: | ---: |
| 10,000 repetitive lines | 254 µs | 41 µs | 584 µs |
| 10,000 varied lines | 256 µs | 41 µs | 592 µs |
| 100,000 repetitive lines | 2.59 ms | 28 µs | 6.40 ms |

The 100,000-line fixture added about 63.7 MiB of resident terminal footprint
before compression and produced a 7.54 MiB snapshot. Its READY prefix was only
10,625 bytes, versus 44,042 bytes for the 10,000-line case, due to different
active-page overlap. This explains why its READY measurement can be smaller;
more history does not inherently make restoration faster. It is not a test of
decoding a 64 MiB compressed snapshot in microseconds.

Reading newly written snapshot files in the 128-terminal compression cases took
approximately 141–142 µs median per terminal to reach READY, with median run p99s
around 218–223 µs. READY plus all history averaged about 0.97–0.98 ms per terminal.
These tests use ordinary buffered file reads with uncontrolled OS cache state;
they do not establish uncached storage latency. The separate single-terminal
100,000-line file test took about 17 ms to restore its history after READY.

For direct parking without prior compression, encoding to a buffered file and
flushing took about 0.54 ms per terminal; `fsync` added roughly 0.10–0.11 ms.
These are local storage measurements without encryption, network transfer,
concurrent PTY output, or a Rust scheduling layer.

## Correctness checked

Every lifecycle run verified the formatted terminal state and exact unfinished
parser continuation after full restoration. Across those runs, 6,351 terminal
restores passed these checks, and no history page was skipped.

A separate fixture covered ground state, split UTF-8, partial CSI, OSC, DCS, and
alternate-screen state. Each case passed byte-for-byte binary re-encoding after
restore, ten additional round trips, subsequent sequence completion, resize, and
terminal-query replies after rebinding host callbacks. Truncated and corrupted
snapshots were rejected. The custom allocator reported zero retained requested
bytes and zero owned mapped bytes after teardown. These checks do not constitute
full VT conformance, fuzzing, or a runtime lifecycle soak.

## Resulting design decisions and limits

Use compression to reduce the cost of terminal history that remains resident.
Use complete binary parking when live model access is unnecessary. Treat buffer
reclamation as part of the design, preserve exact snapshot-to-stream boundaries,
and report READY separately from complete history. Keep the raw replay budget
independent: decoded terminal scrollback cannot replace exact byte replay.

The results support low memory for many parked models, but do not establish a
few-KiB total session footprint or a 500-session capacity claim. Dynamic reader
handoff, client transport/buffer parking, encrypted storage, real mixed PTY
traffic, and Linux execution remain unmeasured here. The prior fixed-reader
comparison remains the evidence for I/O placement. No comparative benchmark of
other multiplexer products was performed.

Scratch source paths, archived harness revisions, exact build command, compiler
versions, and per-series parameters are recorded in the provenance file. The
scratch harnesses remain outside the documentation-only package.
