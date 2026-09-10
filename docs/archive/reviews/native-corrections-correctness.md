# Independent adversarial correctness review: native page-capacity admission and continuation C1 trimming

Reviewed commits `95710a9` ("Reject snapshot page capacities a native page cannot
address") and `81960a2` ("Begin continuation export at the sequence still
unfinished") on branch `native-page-capacity-admission`, on 2026-09-09. Read
AGENTS.md and coding_standards.md. Scope: the corrected vendored Ghostty sources
`src/terminal/snapshot/page.zig`, `src/terminal/stream_continuation.zig` and
`src/terminal/stream.zig`, plus the surrounding admission and ownership paths in
`PageList.zig`, `snapshot/screen.zig`, `snapshot/history.zig`, `c/snapshot.zig`,
`scripts/native/owner.c`, `scripts/native/checkpoint.c` and
`crates/infrastructure/src/terminal/`. `docs/archive/reviews/terminal-page-capacity-admission.md`
and `docs/archive/reviews/terminal-continuation-c1-introducer.md` were treated as
hypotheses to falsify, not as accepted reasoning.

This review ran native Zig tests only. It did not build the static library, did
not run `experiments/run.py`, did not run the Rust workspace or `scripts/gate.py`,
and did not run the corpus. All temporary probes were written into the vendored
tree, executed, then reverted; `python3 scripts/native/verify_source.py
work/experiment-cache/ghostty-82232ecde55405559dec29c5466cb9e39938cb41` exits 0
and the four touched files hash to their reviewed patched values
(`stream.zig` `7e2f63d5…`, `stream_continuation.zig` `8d36a499…`,
`PageList.zig` `a7703d31…`, `snapshot/page.zig` `3d10248e…`). The unfiltered
`zig build test-lib-vt` on the restored tree exits 0.

## Verdict

**I could not break either correction's core claim.** Roughly 800,000 adversarial
continuation cases and 169,344 adversarial page capacities produced zero
counterexamples to claims 1, 2 and 3 as stated. The correctness of the two
patches themselves is not the finding of this review.

The finding is that the page-capacity correction's own framing — that measuring
`Page.layout(capacity).total_size` against `size.max_page_size` is *the*
admission rule for an untrusted PAGE header — is incomplete. It admits a
per-page allocation of up to 2 GiB and a per-record resident cost of ~1 MB with
no byte budget applied anywhere before allocation, and page backing memory does
not pass through the wrapper allocator that carries the runtime's `native_bytes`
cap. That is one P2 below. Two review-document statements are also factually
wrong and must be corrected before either document is cited as accepted
reasoning.

## Method

Command form (from the vendored source directory), pinned Zig 0.16.0, macOS
arm64, Debug build:

```
../zig-aarch64-macos-0.16.0/zig build test-lib-vt -Dtest-filter="<name>" \
  --global-cache-dir <repo>/work/experiment-cache/zig-global-cache
```

Temporary probes, all now removed:

- `ZZPROBE continuation adversarial fuzz` — 200,000 random inputs of 1–12 bytes
  over a 30-symbol adversarial alphabet (ESC, all six C1 introducers, 0x9C, CAN,
  SUB, 0x85, BEL, DEL, UTF-8 lead/continuation bytes, CSI/DCS/OSC/APC openers and
  finals), bulk feed versus byte-at-a-time feed.
- `ZZPROBE continuation adversarial fuzz v2` — 150,000 random inputs of 1–28
  bytes, each replayed under a random chunking of 1–8 feeds and again under a
  random `continuation_max_bytes` drawn from {2, 5, 9, 4096} to exercise
  `extend`, `replace`, `markBroken` and recovery.
- `ZZPROBE continuation structured APC/C1 shapes` — 194,688 constructed inputs
  (prefix × opener × payload × C1/abort byte × opener × payload × C1/abort byte)
  under four chunkings, ≈780,000 checks.
- `ZZPROBE APC-seeded C1 fuzz` — 300,000 inputs seeded with an unfinished
  SOS/PM/APC string followed by up to 38 adversarial bytes, random chunking.
- `ZZPROBE ignore-state and intermediate C1 shapes` — constructed inputs opening
  `dcs_ignore` (`ESC P :`), `csi_ignore` (`ESC [ :`), `escape_intermediate`
  (`ESC SP`), `dcs_param`, `dcs_passthrough` and `osc_string`, each crossed with
  every C1 introducer, 0x9C, CAN, SUB, 0x85, 0x91, 0x99, BEL and LF.

Every continuation case asserted, against the source stream: (a) the exported
suffix is byte-identical under every chunking; (b) `continuationpkg.validate`
accepts it; (c) the export is never longer than the retained bytes and never
longer than `max_bytes`; (d) a stream restored from the export commits nothing
(`handler.committed == 0`); (e) the restored stream's `parser.state` and
`utf8decoder.state` equal the source's; (f) the restored handler's APC active
flag and accumulated APC payload, and the DCS active flag, equal the source's;
(g) the restored stream re-exports the same bytes; (h) feeding an identical
suffix (`ESC \ Z BEL`) to both streams produces the same number of committed
handler actions.

Page probes:

- `ZZPAGE layout is total over extreme capacities` — the full cross product of
  cols {1,2,3,215,1000,65534,65535} × rows {1,2,215,65535} × styles
  {0,1,2,127,128,129,32768,65535} × hyperlink_bytes {0,1,15,16,17,4096,65535} ×
  grapheme_bytes {0,1,3,4,511,512,8192,2^30,2^31−1,2^31,2^32−2,2^32−1} ×
  string_bytes {0,1,3,4,2048,2^30,2^31,0xffff0800,2^32−1} = 169,344 headers.
- `ZZDEGENERATE admitted capacities build usable pages` — every admitted
  degenerate capacity actually constructed with `Page.init`, then exercised
  through `styles.addWithId`, `string_alloc.alloc` and `grapheme_alloc.alloc`
  followed by `assertIntegrity`.
- `ZZFOOTPRINT admitted worst-case page cost` and `ZZBUILDER finish ignores the
  configured byte budget` — see the P2 below.

## Claim 1: page capacity admission — not broken

`Header.pageCapacity` (`snapshot/page.zig:570-600`) rejects every capacity whose
completed layout exceeds `size.max_page_size`, and evaluating `Page.layout` for
an attacker-chosen capacity is safe.

- All 169,344 headers evaluated `Page.layout` under a safety-checked Debug build
  with no panic, no `unreachable`, no integer overflow, no divide-by-zero and no
  `ceilPowerOfTwo` failure. This matters specifically because the shipped library
  is ReleaseFast, where each of those would be undefined behaviour rather than a
  trap; a Debug trap is the correct oracle for that class.
- Every admitted layout satisfied `total_size <= size.max_page_size`,
  `total_size % std.heap.page_size_min == 0` (so `Page.init`'s alignment
  assertion at `page.zig:247` holds), and every recorded member start fit
  `size.OffsetInt`.
- Boundary behaviour is clean: largest admitted layout 4,096,524,288 bytes,
  smallest rejected layout 4,311,695,360 bytes, against
  `max_page_size = 4,294,967,295`. The `@intCast` in `MetaLayout.init`'s grapheme
  count (`page.zig:1847`) and the `grapheme_count` `ceilPowerOfTwo … catch
  unreachable` (`page.zig:1845`) are both reachable with `grapheme_bytes =
  0xFFFFFFFF` and neither traps.
- Zero-valued capacity hints (`style_capacity`, `hyperlink_capacity_bytes`,
  `grapheme_capacity_bytes`, `string_capacity_bytes` all zero) are admitted, and
  `ZZDEGENERATE` confirms the resulting page is constructible and that each
  untrusted-capacity-backed allocator returns an error rather than trapping when
  the payload decoder subsequently asks it for storage. `columns == 0` and
  `rows == 0` are rejected as `InvalidDimensions`, so `initBuf`'s `cap.rows > 0`
  assertion (`page.zig:267`) holds.
- History PAGE decoding shares the fix. `snapshot/history.zig:215` and
  `snapshot/screen.zig:382` both go through `page.Decoder.init`
  (`snapshot/page.zig:193-201`), which calls `pageCapacity` before either
  `PageList.allocatePage` or `Builder.allocatePage` runs. The module-level
  `page.decode` (`snapshot/page.zig:172`) and the test-only `decodePayload`
  (`snapshot/page.zig:347`) also call it before allocating.
- The C boundary maps `CapacityTooLarge` to `.invalid_value` through the `else`
  arm at `c/snapshot.zig:507`, and `rt_restore` (`scripts/native/checkpoint.c:19-36`)
  turns any non-success into `*error = -1` unless the bounded allocator denied,
  so Rust reports `CorruptCheckpoint`. The record is rejected, the owner is
  freed, and no terminal is produced.

One low-severity structural note. `Decoder.capacity()` (`snapshot/page.zig:204-206`)
is `self.header.pageCapacity() catch unreachable`. That was already true before
this change, but its precondition is now richer: previously only nonzero
dimensions, now also a layout bound. `Decoder` is `pub` with `pub` fields, so a
future caller that constructs a `Decoder` without `init`, or mutates `header`
between `init` and `capacity`, converts a decode error into ReleaseFast
undefined behaviour. Returning the capacity computed during `init` from a stored
field would remove the reliance entirely. Worth noting, not a defect today.

## P2: an admitted capacity is still an unbounded allocation

The correction's stated admission rule is the `max_page_size` layout bound. That
bound is a *representability* bound, not a *resource* bound, and nothing else
bounds native page memory before it is allocated.

Measured on this platform with `ZZFOOTPRINT`:

| header fields | admitted layout | resident cost |
| :--- | ---: | ---: |
| `cols=80 rows=24 string_capacity_bytes=0x7fff0000` | 2,155,823,104 B | lazy |
| `cols=1 rows=65535` | 1,064,960 B | 524,280 B of row headers written by `initBuf` |

Both are accepted by `Header.pageCapacity`, and the probe allocated the 2 GiB
page for real: `TerminalPage.init` succeeded and reported `memory.len =
2155823104` from a twenty-byte header field.

Three facts compose:

1. `snapshot/screen.zig:381-385` loops `for (0..header.page_count)` and calls
   `builder.allocatePage(decoder.capacity())` for each record. `page_count` is a
   `u16` (`snapshot/screen.zig:895`), so up to 65,535 pages are allocated and
   held simultaneously.
2. `PageList.Builder.finish` (`PageList.zig:7559-7626`) never compares
   `self.page_size` against `self.options.max_size`. It sets
   `limits.set(.bytes, self.options.max_size)` on the finished list and returns.
   Probe `ZZBUILDER finish ignores the configured byte budget` builds eight pages
   with `max_size = 1` and finishes successfully with `page_size = 3,145,728`
   against `limits.max(.bytes) = 786,432` — four times the configured budget,
   accepted. `screen.decode` is the only caller that passes
   `max_scrollback_bytes` into that field, so the live-screen restore path has no
   byte admission at all. (The history path is different: `PageAllocation.prepend`
   at `PageList.zig:4455` does reject `MaxSizeExceeded`, but only *after* the
   page has been allocated, one page at a time.)
3. Page backing memory does not pass through the wrapper's bounded allocator.
   `PageList.pageAllocator` (`PageList.zig:571-586`) returns
   `mach.taggedPageAllocator` on Darwin and `std.heap.page_allocator` elsewhere;
   the embedder allocator is used only on freestanding targets. The
   `native_bytes` cap enforced by `bounded_alloc` in `scripts/native/owner.c:9-24`
   — documented in `crates/domain/src/terminal/mod.rs:44` as "Hard cap on
   requested native allocation bytes" and defaulting to 8 MiB — therefore does
   not bound the dominant native allocation in a restore.

Concretely, with the default `checkpoint_bytes = 8 MiB`, a checkpoint of roughly
2.2 MB carrying 65,535 minimal PAGE records each declaring `cols=1, rows=65535`
asks for about 69 GB of mappings, of which about 34 GB is resident because
`Page.initBuf` writes every row header. Nothing rejects it before allocation;
the failure mode is allocator exhaustion, reported as `OutOfMemory` →
`.out_of_memory` → `CorruptCheckpoint`. That is a bounded-failure denial of
service rather than memory corruption, and it is not introduced by `95710a9` —
but it is squarely inside the admission question that commit claims to have
settled, and coding_standards.md requires bounding "all session/observer/work
admission before allocation".

Minimum credible fix: charge the declared layout size against the configured
byte budget inside `Decoder.init` or immediately before `allocatePage`, for both
the SCREEN and HISTORY paths, and reject the record before any mapping. A
`page_count` × per-page ceiling check would also work. I did not verify the
end-to-end reachability through `rt_restore` with a crafted snapshot; the
evidence above is native-level and code-level, so the parent should confirm
reachability before grading.

## Claim 2: continuation replay start and ESC spelling — not broken

`Tracker.replayStart` and `Tracker.write` (`stream_continuation.zig:189-283`)
were attacked along every axis named in the assignment. No counterexample.

What the parse table actually guarantees, verified against `parse_table.zig`:

- The exit actions that count as committed work exist only for `osc_string`
  (`osc_dispatch`), `dcs_passthrough` (`dcs_unhook`) and `sos_pm_apc_string`
  (`apc_end`) — `Parser.zig:275-282`. `osc_string` overrides 0x20–0xFF as
  `osc_put` (`parse_table.zig:358`) and `dcs_passthrough` and `dcs_ignore`
  override 0x80–0xFF as payload (`parse_table.zig:265`, `parse_table.zig:215`),
  so no C1 introducer can exit those two. The review document's claim that
  `sos_pm_apc_string` is the only such state is correct.
- `Parser.next` emits exit and entry actions only when `self.state !=
  next_state` (`Parser.zig:275`, `Parser.zig:288`). Therefore 0x98, 0x9E and 0x9F
  *inside* `sos_pm_apc_string` are complete no-ops: no `apc_end`, no `apc_start`,
  no state change. `replayStart`'s `before != scanner.parser.state` guard
  correctly declines to treat them as introducers, and `write` leaves them in the
  payload where replay reproduces them identically. This was my strongest
  candidate counterexample (an APC re-introduced by a raw C1 while already in
  `sos_pm_apc_string`, which would have been classified `.omittable` and silently
  merged two APC payloads); the parse table's same-state suppression makes it
  unreachable. 300,000 APC-seeded fuzz cases with the APC-payload oracle confirm
  it empirically.
- 0x9C (ST), 0x18 (CAN) and 0x1A (SUB) all transition to `.ground` from every
  state that does not override them (`parse_table.zig:62-68`). They therefore
  end the retained window at ground, where `Stream.trackContinuation`
  (`stream.zig:578-593`) resets the tracker, or leave only a UTF-8 tail that the
  `.utf8` `replace` path re-seeds at the lead byte. I could not construct a
  retained buffer that carries committed work past one of them into a
  still-unfinished state.
- C1 introducers inside `dcs_ignore` and `osc_string` are payload and never
  match `entry == scanner.parser.state`, so trimming never starts inside a
  string body.
- The `committed` gate is safe in both directions. When nothing commits, the
  untrimmed retained bytes are exported unchanged and still satisfy
  `validate`'s canonicality check, because a retained buffer can only begin at an
  ESC (from `findVTReplayStart`) or at a UTF-8 lead byte ≥ 0xC0 (from
  `findUtf8ReplayStart`), and any feed containing a later ESC takes the `replace`
  path rather than `extend`. The existing golden `"text\x1b[12\x9d2;title"`
  expectation is preserved for exactly this reason. When something does commit
  and the parser is still building a sequence, trimming to the last introducer
  removes it. I specifically hunted for "committed, still unfinished, last
  introducer at index 0" and for "committed, ends at ground with UTF-8 pending,
  so `replayStart` bails to `.{}` while the retained bytes still carry the
  commit" — neither is reachable, because the `.utf8` append path re-seeds at the
  lead byte whenever the lead is in the current feed, and a pending codepoint's
  lead is always within the last three bytes that `findUtf8ReplayStart` scans.
- A C1 introducer at index 0 of the retained bytes cannot be selected. The
  `replayStart` scanner starts at ground, and at ground every byte goes through
  the UTF-8 decoder rather than the parser (`stream_continuation.zig:437-452`),
  so a leading 0x90/0x9B/0x9D/0x98/0x9E/0x9F leaves `before == after == .ground`
  and never satisfies the introducer test. The two-byte ESC spelling therefore
  always replaces a byte at index ≥ 1.
- `broken` and `max_bytes` interaction: 150,000 cases were replayed with
  `continuation_max_bytes` of 2, 5 and 9 across random chunkings. Where the
  tracker broke, `writeContinuation` returned `ContinuationUnavailable` as
  before; where it did not, every assertion above held and the export never
  exceeded the configured cap.
- Chunking independence and idempotence were extended to all of the new cases,
  as the assignment asked: bulk, byte-at-a-time and random 1–8-chunk feeds all
  produce identical exports, and the restored stream re-exports byte-identically.

## Claim 3: export size — not broken

The exported continuation cannot exceed `max_bytes` or the decoder's
`max_continuation_bytes`.

- Export length ≤ retained length, proved above (C1 index ≥ 1, one byte replaced
  by two, everything else only omitted) and asserted on every fuzz case as an
  explicit `GROWTH` check. Retained length ≤ `max_bytes` by `replace`/`extend`
  (`stream_continuation.zig:163-186`).
- The two limits are the same number at runtime. `rt_restore`
  (`scripts/native/checkpoint.c:19-32`) passes the single `continuation`
  parameter both to `GHOSTTY_SNAPSHOT_DECODER_OPT_MAX_CONTINUATION_BYTES` and to
  `rt_configure`, and both `rt_new` and `rt_restore` receive it from
  `config.continuation_bytes` (`crates/infrastructure/src/terminal/mod.rs:36`
  and `:69`). `continuation.decode` rejects `len > max_bytes` before allocating
  (`snapshot/continuation.zig:94`), so a checkpoint written by a terminal with
  budget N is always decodable by a terminal configured with budget N.

Worth noting as a format property rather than a defect: the exported bytes are
no longer necessarily a byte-suffix of the original PTY stream, because a C1
introducer is respelled as ESC + (byte − 0x40). `snapshot/continuation.zig:22`
describes the payload as "Canonical replay-safe PTY bytes", and
`snapshot/snapshot.ksy` describes the record for external consumers. The
substitution is semantics-preserving for Ghostty's own parser (and Ghostty
conflates SOS/PM/APC, so 0x98/0x9E/0x9F → `ESC X`/`ESC ^`/`ESC _` loses nothing
it distinguishes), but any consumer that assumed the continuation is a literal
tail of the byte stream now needs updating. The upstream report should say so
explicitly.

## Rust adapter soundness after a rejected snapshot

No P1/P2 found.

- A rejected restore is clean. `rt_restore` frees the owner and returns NULL on
  any decoder failure (`scripts/native/checkpoint.c:30-31`), and
  `GhosttyTerminalFactory::restore` (`crates/infrastructure/src/terminal/mod.rs:64-90`)
  produces no terminal. If `rt_restore` succeeds but the post-restore size check
  fails, `terminal` is dropped and `Drop` calls `rt_free`, so the native owner
  and decoder are released exactly once.
- Borrowed checkpoint bytes outlive the decoder. `restoring: Some(checkpoint)`
  holds the `Vec` whose pointer the decoder retains, and
  `restore_history_step` clears `restoring` only on result 1, which is the branch
  where `rt_history` has already called `ghostty_snapshot_decoder_free`
  (`scripts/native/checkpoint.c:45`). A mid-history rejection sets `failed` and
  leaves both the decoder and its source alive until `Drop`.
- A rejected PAGE inside `decodePage` frees the detached allocation through
  `allocation.deinit()` before the error escapes (`snapshot/history.zig:214-221`),
  so the receiving `TerminalScreen` is unchanged, matching the documented
  contract.

Two accuracy corrections to the continuation review document, both of which
change the stated severity of the original defect:

1. `docs/archive/reviews/terminal-continuation-c1-introducer.md` states that "`state.rs`
   converts it to `TerminalError::EngineFailure` while setting `failed = true`,
   so the terminal can never be checkpointed again". `ITerminal::checkpoint`
   (`crates/infrastructure/src/terminal/state.rs:47-78`) does **not** set
   `failed`; only `mutation()` (`mod.rs:118-127`) and `restore_history_step` do,
   and `checkpoint` does not call `mutation`. In the application layer
   `native_call` (`crates/application/src/projection/native.rs:235-246`) latches
   only on panic, not on a `TerminalError`. So the real impact of the seed-509
   defect was: every checkpoint attempt fails with `EngineFailure` for as long as
   the offending retained window persists, and it clears as soon as a later feed
   reaches ground or contains a new replay start. That is still a genuine defect
   worth fixing, but the "permanently unable to checkpoint" framing is wrong and
   should not be repeated in the upstream report.
2. The same document's failure section should note that the *reported* error
   category is wrong for a related, still-live case. When the tracker breaks
   because an unfinished sequence exceeds `continuation_bytes`,
   `writeContinuation` returns `ContinuationUnavailable`, which
   `c/snapshot.zig:502-505` maps to `.invalid_value` and `rt_checkpoint` reports
   as `-1`, so a pure budget overrun surfaces as `EngineFailure` rather than
   `BudgetExceeded`. `crates/infrastructure/tests/terminal_bounds.rs:133-142`
   pins only `is_err()`, so it does not distinguish them. Pre-existing, low
   severity, but it is the same confusion of "engine broke" with "budget
   exceeded" that made the seed-509 diagnosis harder.

One further note on the wrapper, unrelated to either commit: `o->denied` is
sticky for the life of the owner (`scripts/native/owner.c:14-22`, and the header
comment says so). A native-allocator denial that occurs during a read-only
`rt_checkpoint` therefore poisons the owner, and the *next* `rt_feed` returns
`-2`, which `mutation()` converts into a latched `failed`. That is a real path
by which a checkpoint-time budget event permanently disables a terminal — just
not the path the review document describes.

## What I did not probe

- ReleaseFast codegen. Every probe ran in the Debug `test-lib-vt` build. That is
  the correct oracle for detecting would-be undefined behaviour, but it does not
  prove the shipped ReleaseFast library behaves as measured.
- Non-Darwin targets. `findVTReplayStart`'s SIMD lane width, `std.heap.page_size_min`
  (16,384 here) and `PageList.pageAllocator`'s branch all differ on Linux/x86-64,
  which changes the exact admitted-layout ceiling and the vector-boundary cases.
  The Linux gate remains outstanding as the documents state.
- End-to-end reachability of the P2 through `rt_restore` with a crafted
  multi-page snapshot. The evidence is native-level and by code inspection.
- The Rust workspace, `scripts/gate.py`, coverage, and the corpus. None were run.
- `nextSliceUntilGround`'s tracking call site (`stream.zig:628-630`,
  reached from `c/terminal.zig:929`) was reasoned about — it feeds
  `trackContinuation` a prefix that ends exactly at ground, which is equivalent
  to a chunk boundary my fuzz already covers — but not exercised directly.
- Continuation behaviour under a non-standard handler. `stream_continuation.zig:99`
  scopes the guarantee to `TerminalStream`; I did not test `vtRaw` interception.
- `style_count` / `hyperlink_count` far exceeding their declared capacities
  through the real payload decoder. `ZZDEGENERATE` shows the allocators fail
  gracefully at the unit level, but I did not drive `decodePayloadBody` with a
  crafted table.

## Findings by severity

**Must fix before acceptance**

- P2 — admitted page capacities carry no byte budget before allocation.
  `PageList.zig:7559-7626` (no `max_size` check in `Builder.finish`),
  `snapshot/screen.zig:381-385` (up to 65,535 pages allocated up front),
  `PageList.zig:571-586` (page memory bypasses the `native_bytes` allocator).
  Measured worst cases: a 2,155,823,104-byte page from a twenty-byte header, and
  ~34 GB resident from a ~2.2 MB checkpoint.
- P2 — `docs/archive/reviews/terminal-continuation-c1-introducer.md` misstates the
  failure's persistence (`failed = true` is not set by `checkpoint`). Correct the
  document and the upstream discussion text before either is cited as accepted
  reasoning; an ADR proof ledger entry that overstates a defect is itself a
  blocker under coding_standards.md.

**Worth noting**

- `Decoder.capacity()`'s `catch unreachable` now depends on a richer
  precondition on a `pub` struct with `pub` fields; store the capacity computed
  in `init` instead.
- The exported continuation is no longer a literal suffix of the PTY stream;
  say so in `snapshot/continuation.zig`'s doc comment and in the upstream report.
- A continuation budget overrun is reported as `EngineFailure`, not
  `BudgetExceeded` (`c/snapshot.zig:502-505`); `terminal_bounds.rs:133-142` does
  not distinguish them.
- `o->denied` stickiness lets a checkpoint-time allocator denial latch the
  terminal on the next feed (`scripts/native/owner.c:14-22`).
- Export now runs two full `BoundaryScanner` passes over the retained bytes
  instead of one. Export-time only and bounded by `continuation_bytes`;
  negligible, but it is a real doubling of that path's cost.

No unresolved P1 was found in the reviewed scope. The two corrections themselves
are, as far as this review could determine, correct.
