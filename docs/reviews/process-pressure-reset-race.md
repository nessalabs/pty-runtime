# Linux pressure failure: clean retirement followed by socket reset

2026-09-08. Independent diagnosis of the root's Linux gate failure; the reviewer
then authored the bounded shared-protocol correction. The root subsequently independently re-reviewed the shared transport correction
and its regression tests; the next complete gate remains root-owned. No terminal/native, coverage
fixture, producer-count or output-oracle changes were made in this investigation.

## P2 reproduced and diagnosed

On the exact frozen `/tmp/pty-loop4-gate` source, the unchanged
`simultaneous_producers_deliver_complete_bytes_and_quiet_cancel_remains_live`
failed immediately when rerun alone (0.13 seconds). It runs 16 producers of
131072 zero bytes each plus one quiet cancellation session. The evidence is
`docs/verification/process-pressure-race/linux-first.json`.

Scratch-only tracing identified the host read-error branch at
`crates/infrastructure/src/process/guardian.rs:148-153`. The affected producer
already had all 131072 bytes, exit code 0 and actual EOF. Helpers emitted no Fault.
The decisive trace (`linux-error-detail.log`) shows `ConnectionReset`, Linux errno
104, on the guardian channel **after `retiring=true`**. A late host Release remains
unread when the helper sends its final Retiring frame and closes. Linux delivers
buffered status bytes and then reports reset; Darwin reports EOF. The host already
accepts a normal EOF after validated retirement, but the codec surfaced reset as
an unconditional supervision error.

The first tracing attempt that printed every frame passed, so it is explicitly
not negative evidence. Red reproduction returned after reducing instrumentation
to fault/error call sites. All scratch tracing and fixture diagnostics were
reverted before final verification. Trace contents contain only protocol facts,
PID identifiers and counts, not workload commands/environment/output bytes.

## Regression first, then shared correction

Two real `UnixStream::pair` regressions were added to `scripts/guardian/protocol.rs`
before production behavior changed. One queues a late control without consuming
it, sends one complete final Retiring record from the peer, then closes the peer;
it requires the record followed by EOF. The other truncates that final frame and
requires InvalidData. Both failed on Linux before the fix (`linux-protocol-red.log`):
reset escaped directly, including instead of the truncation classification.

The same protocol file now routes ConnectionReset and ordinary EOF through
`finish_receive`. No pending bytes means transport EOF; a partial incoming frame
remains InvalidData. Other read failures retain their existing error behavior.
There is no target-OS branch and no socket errno or lifecycle correction in a
consumer. The file remains under 350 nonblank lines.

EOF is only a transport fact. Existing host `channel_ended` still reports
supervision loss if that channel has not supplied validated Retiring; reset does
not grant retirement permission or manufacture workload exit status. The final
forced guardian/sentinel-loss tests continue to require explicit supervision
failure, and pass. Partial-frame errors remain faults even after a retirement
record. This preserves the authority boundary rather than ignoring read errors.

## Author verification and source freeze

Only production/regression file changed: `scripts/guardian/protocol.rs`.
`docs/verification/process-pressure-race/source.json` records canonical and
regression-before-fix hashes. Remote verification records its exact final protocol
hash and platform; all temporary tracing was removed before that run.

Both macOS arm64 and Linux x86_64/glibc 2.39 pass:

- All five shared helper protocol tests, including the two new real socket cases.
- Infrastructure process contract, pressure, guardian-loss and changing-foreground
  integration suites, with the original pressure workload and assertions intact.
- Public raw completion contract preserving actual signal exit and descendant
  drain distinctions.

Scoped helper Clippy passes on macOS. Raw output is retained in the
`macos-*.log` files and `linux-fixed-verification.{json,log}` under the same evidence
directory. Linux additionally passes 20 unchanged pressure repetitions after the
initial complete scoped run: 320 producer workloads plus 20 quiet sessions, with
exact bytes/exit/drain/failure oracles. `linux-pressure-repeats.jsonl` records each
result and end-to-end command duration (0.186–0.239 seconds). These timings include
Cargo/process startup and are scoped repetition evidence, not release latency or
throughput measurements. They do not replace full-load/soak qualification.

Disposition: the demonstrated P2 is resolved in scoped re-review. The root
independently inspected Channel::receive/finish_receive and the socketpair tests,
confirmed partial-frame rejection and higher-level retirement validation remain
intact, and reported no blocker. Portable scoped regressions pass. A complete
source-bound gate remains necessary before repository acceptance; none was
dispatched by this reviewer while the root coordinates concurrent work.
