# Native snapshot corrections: organization and design-pattern review

Independent organization/design review of exactly two commits on
`native-page-capacity-admission`: `95710a9` "Reject snapshot page capacities a
native page cannot address" and `81960a2` "Begin continuation export at the
sequence still unfinished". This is not a self-review; the author owns the
diagnoses, and DDD and behavioral correctness are reviewed separately.

This document replaces an earlier draft of the same review that was interrupted
mid-correction. Every claim below was re-derived from the sources. The draft's
errors that I had to correct are listed in the final section rather than quietly
overwritten, because several of them inverted a conclusion.

## What I verified, and what I did not

Verified by reading the artifact: both diffs in full; the 613-line patch
(19 hunks, +447/-12, across six files); `scripts/native/verify_source.py`;
`scripts/native/patches/README.md`; `scripts/gate.py`;
`scripts/tests/test_native_source.py`; the corrected vendored sources under
`work/experiment-cache/ghostty-82232ecde55405559dec29c5466cb9e39938cb41/src/terminal/`
(`stream_continuation.zig`, `stream.zig`, `snapshot/page.zig`, `Parser.zig`,
`parse_table.zig`, `snapshot/snapshot.zig`, `c/terminal.zig`,
`stream_terminal.zig`); the full call graph behind the author's
performance-placement claim; both diagnosis docs; `docs/upstream/`; and the
layout of all 67 directories under `docs/verification/`, all 36 `source.json`
files, and the `metadata.json` schema, against the two new evidence directories.

I recomputed the SHA-256 of every file in the identity table below and confirmed
each against `verify_source.py` and the README table.

Not done: no build, no `zig build test-lib-vt`, no gate run, no corpus run. I did
not re-derive the seed 201 or seed 509 failures and did not rebuild the library
to re-verify `library_sha256`. I read the recorded logs but re-ran nothing.
Nothing here converts the author's scoped evidence into acceptance.

## 1. The vendored-patch mechanism

The mechanism is now one fixed patch carrying seven defects across six files: 19
hunks, six hash pairs in `verify_source.py:14-33`, a 13-row hash table in the
README (`:59-73`, twelve file rows plus the patch row), and six superseded-identity
paragraphs (`README.md:105-139`). Counting the current identity, the patch has
been through seven identities and six rotations.

The README's own statement of intent — "This is a fixed patch for four roundtrip
defects, one allocator capacity defect, one decoder capacity-admission defect,
and one continuation-export defect, not an extensible patch mechanism"
(`scripts/native/patches/README.md:97-99`) — is no longer true as written. Seven
defects arriving one at a time over six rotations *is* an extension history. That
sentence should be replaced with the policy actually in force rather than defended.

My recommendation is nevertheless to **keep the single fixed patch**, and to
mechanise the ritual around it rather than restructure it.

**Splitting per-defect is worse.** `PageList.zig` carries six hunks and
`snapshot/page.zig` five, contributed by different corrections, so N patch files
would need a strict application order, would conflict on shared context at the
next upstream rebase, and would still have to compose into one identity for the
compatibility marker and the build stamp. The per-defect provenance a split would
buy mostly exists already — though not cleanly: six of the seven defects have a
`docs/reviews/terminal-*.md` diagnosis, and the seventh (the bitmap allocator
capacity defect) is documented instead in `docs/reviews/bitmap-capacity-independent.md`
and `bitmap-capacity-integration-review.md`, neither of which appears in the
README's diagnoses list at `:99-106`. Fix that omission rather than the patch shape.

**Vendoring corrected sources is also worse.** The patch is the reviewable
artifact: +447/-12. Six full Zig files (`stream_continuation.zig` alone is 688
lines, `stream.zig` 5,423) would bury that delta and complicate the third-party
notice.

**The real cost is not the patch — it is that the identity is replicated by hand
into three places and only two are checked:**

- `scripts/native/verify_source.py:38` `PATCH_SHA256` — checked against the patch
  file (`:56`) and against the build stamp (`:105`).
- `scripts/native/patches/README.md:61` table row — hand-edited, unchecked.
- `crates/infrastructure/src/terminal/mod.rs:13` `COMPATIBILITY` — hand-edited,
  unchecked (finding 1).

Concretely: (i) add a gate check that the `COMPATIBILITY` literal embeds
`PATCH_SHA256`; (ii) generate the README hash table from `verify_source.py`
instead of hand-editing it; (iii) move the six superseded-identity paragraphs
(`README.md:105-139`) into a `HISTORY.md` so the operating README stays an
operating document; (iv) generalize `PREVIOUS_PAGE_LIST_SHA256S`
(`verify_source.py:34`, special-cased at `:78-79`) into a per-file dict of
accepted intermediate states. That is a small amount of scripting and it removes
the three manual steps a future correction can silently get wrong. **The
mechanism has outgrown its description, not its shape.**

One strategic coupling I hand to the DDD/correctness reviewers rather than
resolve here. The runtime compatibility marker is keyed to the patch hash, so it
conflates "the native source changed" with "the checkpoint contract changed".
For `81960a2` the bump is genuinely earned: `write` now emits different bytes for
the same retained state, so the encoded form moved. For `95710a9` it is not:
`pageCapacity` (`snapshot/page.zig:570-600`) only rejects capacities whose
assembled layout exceeds `size.max_page_size`, which `Page.init` already asserts,
so no previously written *valid* checkpoint can become undecodable — yet that
commit discarded every existing checkpoint anyway. If more native defects are
expected before release, a hand-assigned codec generation, bumped only when the
encoded/replayed contract actually moves and carrying the patch hash alongside
for provenance, would decouple correction churn from checkpoint churn.

## 2. The new Zig in `stream_continuation.zig`

The correction fits the module's level of abstraction. It reuses `BoundaryScanner`
— the module's existing "replay one byte the way Stream would" abstraction
(`:390-424`) — instead of introducing a second parser model, which is exactly right
given that a second model is what caused the defect. The C1 respelling in `write`
(`:271-280`) feeds the synthesized `ESC` + final into the scanner before emitting
them, so the remaining classification stays exact rather than approximated. That
is careful work.

**The two-pass scan at export is the right structure.** One pass cannot produce
both answers: the trim decision needs the classification of the *whole* retained
buffer (`replayStart` accumulates `committed` over every byte, `:216-237`), while
the omission decision must be reclassified from ground at the trim point, because
the exported suffix replays from ground (`write` re-inits a scanner at `:269`).
Cost is two scans of at most `continuation_max_bytes`, paid per continuation write.

### The feed-path claim: verified, with one correction to its scope

The commit message says "The feed path and its SIMD scan are untouched; the new
analysis runs only at export." I verified the first half from the patch and the
second from the call graph.

Feed path untouched — confirmed. The `stream_continuation.zig` hunk is a single
hunk (`@@ -186,17 +186,99 @@`) covering only `write`'s doc comment, the three new
private items, and `write`'s body. `Tracker.append` (`:152-161`), `replace`
(`:164-175`), `extend` (`:180-187`) and the SIMD `findVTReplayStart` (`:304-355`)
are byte-identical. The feed entry point is `Stream.trackContinuation`
(`stream.zig:578-596`), which calls `tracker.append` and nothing new. No per-byte
cost was added on ingest.

"Only at export" — **imprecise, and the draft I replaced got the call graph
backwards.** `replayStart` has exactly one caller, `Tracker.write` (`:268`);
`write` is reached only from `Stream.writeContinuation` (`stream.zig:565`). Its
callers are:

- `c/terminal.zig:958` `continuationWriteIo` — the export funnel, itself reached
  from the four public output forms at `:993`, `:1028`, `:1057`, `:1066`.
- `c/terminal.zig:761` `restoreContinuation` — the **restore** path, which
  re-exports and compares for byte identity (`:766`).
- Everything in `snapshot.zig` (`:941`, `:976`, `:1054`, `:1062`, `:1104`,
  `:1142`, `:1160`, `:1163`) and `stream_terminal.zig:5663` is **test** code —
  those files' first `test` blocks are at `snapshot/snapshot.zig:713` and
  `stream_terminal.zig:1927`, well above every one of those lines.

So the new scan runs on continuation *write*, which includes restore
verification, not export alone. The load-bearing part of the claim — that nothing
was added to the per-byte ingest path — holds. But see finding 8: `continuation_buf`
can call `continuationWriteIo` twice in one API call, making it four scans.

### The two diagnosis claims, checked against the parser tables

Both confirmed:

- `sos_pm_apc_string` is the only state with a committed exit action that does
  not override the anywhere C1 transitions. Its block (`parse_table.zig:111-120`)
  sets only `0x00`-`0x7F`, completing at `:119` (`range(&result, 0x20, 0x7F, ...)`),
  so the anywhere C1 entries at `:74-85` survive. `osc_string` (block `:349`)
  overrides `0x20`-`0xFF` at `:358`; `dcs_passthrough` (block `:241`) overrides
  high bytes from `:252` with an explicit rationale. The three states with
  committed exit actions are exactly `osc_string`, `dcs_passthrough` and
  `sos_pm_apc_string` (`Parser.zig:276-281`).
- A C1 introducer can never be at index zero of the retained bytes. `replace` is
  only ever called with `input[start..]` where `start` came from
  `findVTReplayStart` (an `ESC`) or `findUtf8ReplayStart` (a byte `>= 0xC0`), at
  `:154-160`; every C1 introducer is below `0xC0`. This is load-bearing beyond
  bounds safety: it means the exported length is `len - index + 1 <= len`, so a
  respelled export can never exceed `continuation_max_bytes` and a restored stream
  cannot be broken by re-feeding its own export.
- The `before != state` guard (`:232-233`) correctly declines `0x98/0x9e/0x9f`
  from inside `sos_pm_apc_string`: `Parser.next` suppresses both the exit action
  (`Parser.zig:275`) and the entry action (`:288`) on a self-transition, so no
  `apc_end` is committed there and there is nothing to trim.

### Placement is where I disagree

`ReplayStart` (`:190`), `replayStart` (`:213`) and `c1Introducer` (`:247`) are
nested inside `Tracker`, but all three are pure functions of `[]const u8` — none
touches `self` beyond `self.bytes.items`. The module's two sibling scan helpers,
`findVTReplayStart` (`:304`) and `findUtf8ReplayStart` (`:369`), are file-scope
free functions, each with its own focused unit test (`:555`, `:581`). Nesting the
new scan in `Tracker` is what makes it untestable without an allocator and a
constructed Tracker, and is the proximate cause of findings 5 and 6.

## 3. Test placement and shape

`snapshot/page.zig:1363` mirrors its sibling `"decode validates dimensions"`
(`:1326`) exactly — a table of `Header` values, `encode` into a fixed writer, then
`expectError` from `decodePayload`. Correct file, correct neighbour, correct shape.

The two continuation tests (`stream.zig:4950`, `:5002`) sit among the ten
pre-existing `"stream: continuation ..."` tests (`:4839`, `:4913`, `:5021`,
`:5086`, `:5142`, `:5179`, `:5235`, `:5276`, `:5311`, `:5344`) and use the same
`S.init` / `nextSlice` / fixed-writer idiom. Placing them in `stream.zig` rather
than `stream_continuation.zig` is right — they need `Stream` and
`ContinuationTestHandler` (`stream.zig:4637`), and `stream_continuation.zig`
deliberately does not import `stream.zig` (rationale at `:384-385`; confirmed —
its only imports are `std`, `quirks.zig`, `Parser.zig`, `UTF8Decoder.zig`,
`:1-5`). But it means `stream.zig` joined the patch surface *solely to carry
tests*: one hunk, +71/-0, buying a sixth `TARGET_HASHES` entry, a README row, and
a permanent hash to rotate, for zero production change. Right call; the cost
belongs in the mechanism ledger above.

The gap is that the module's own convention — every helper gets a focused unit
test in its own file, including `Tracker` itself (`:599`, `:630`, `:667`) — was
not followed. `stream_continuation.zig` gained no test at all. All coverage of
`replayStart` and `c1Introducer` is indirect, through `stream.zig`. See findings
5, 6 and 9.

## 4. `docs/upstream/`

Committing the upstream bug-report drafts belongs here. They are the exit strategy
for the local fork — `README.md:143` states the corrections remain local until
upstream resolves them — and the "Notes before posting" sections record real
reporting constraints (Ghostty's mandatory AI disclosure, the vouch requirement
before PRs, the prior-art search actually performed, and in the C1 draft an
explicit design question to ask upstream rather than assert). Losing that to a
chat transcript would be worse. One directory, two files, is proportionate; I
would not restructure it.

It is under-organised in two ways, both cheap: the drafts still read as unposted
while the repo asserts they are filed (finding 10), and the filenames encode
upstream's discussion *category* (`ghostty-issue-triage-`), which rots if upstream
renames it — `ghostty-page-capacity.md` with the category named inside the file
would age better. I checked the third complaint in the draft I replaced (that the
directory is unindexed) and **withdrew it**: `docs/README.md` indexes ADRs and
experiments only, and indexes neither `docs/reviews/` (108 files) nor
`docs/verification/`. `docs/upstream/` is being treated exactly like its peers.

## 5. Evidence layout

Consistent with the existing convention, with three exceptions.

Both new directories follow the shape used by `docs/verification/resumed/`: one
subdirectory per run containing `command.log` plus a `metadata.json`. I diffed the
schema: `page-capacity/seed201/metadata.json`,
`continuation-c1/macos-gate/metadata.json` and `resumed/native-seed201/metadata.json`
carry the identical eleven keys (`architecture`, `base_revision`, `command`,
`elapsed_seconds`, `exit_code`, `passed`, `platform`, `rustc`, `source_files`,
`sources_unchanged`, `started_unix`) and the same 301-entry `source_files`
inventory. Targeted native runs appear as bare `.log` files beside a `source.json`
pinning the patch, the verifier, the build stamp and the corrected sources —
matching `bitmap-capacity-independent/`.

Correcting the draft I replaced on two counts:

- **A new convention *was* invented.** The `note` field in
  `page-capacity/native-tests/source.json` and
  `continuation-c1/native-tests/source.json` appears in exactly those two of the
  36 `source.json` files in `docs/verification/`. `bitmap-capacity-independent/source.json`
  has no `note` and a wholly different schema (`before_sha256`,
  `scratch_after_sha256`, `canonical_unchanged`, `patch_sha256`, `scratch_root`,
  `tests`). The invention is a *good* one — it is exactly the "explain what the
  mutation removed" the standards ask for — and I would keep and propagate it.
  But it should be recognised as new, not as conformance.
- **A missing `README.md` is not a deviation.** Of 67 directories under
  `docs/verification/`, 24 have a `README.md` and 43 do not — including
  `bitmap-capacity-independent/`, `loop2/`, `loop3/`, `foundation/` and
  `packed-pages/`. The two new directories match the majority. I withdrew the
  draft's finding on this entirely.

The three real exceptions are findings 2, 3 and 4.

## Findings

### Must fix before this scope is called passed

1. **P2 — the compatibility marker is replicated with no mechanical check.**
   `crates/infrastructure/src/terminal/mod.rs:13` embeds the patch SHA-256 as a
   string literal (`snapshot-wrap-745b5e98...`); `scripts/native/verify_source.py:38`
   holds the same value. I read `scripts/gate.py` in full and
   `scripts/tests/test_native_source.py`: nothing asserts they agree. A missed
   rotation silently lets checkpoints written by a superseded codec decode — the
   exact failure `README.md:91-93` says the marker prevents, and the marker is
   enforced at `mod.rs:56` and `state.rs:52`. Both commits rotated it correctly,
   but that is process, not enforcement. Add a check that the literal contains
   `PATCH_SHA256`.

2. **P2 — cited evidence file is empty.**
   `docs/verification/page-capacity/native-tests/full-green.log` and
   `docs/verification/continuation-c1/native-tests/full-green.log` are both **0
   bytes**, in the commits and on disk. Yet
   `docs/reviews/terminal-continuation-c1-introducer.md:96` cites "the unfiltered
   `zig build test-lib-vt` exits 0 (`full-green.log`)";
   `terminal-page-capacity-admission.md:93-94` makes the same assertion attributed
   to the directory as a whole; and
   `page-capacity/native-tests/source.json`'s own `note` states "filtered-green.log
   and full-green.log ran on the restored source recorded above." Zero bytes records
   no command, no exit code and no timestamp. Every other run in this tree records
   all three — `filtered-green.log:1` opens with "Build Summary: 46/46 steps
   succeeded; 65/65 tests passed". Capture the invocation and exit status, or delete
   the file and stop citing it. The standards require "command, platform, source
   revision, raw evidence, result".

3. **P2 — the positive case in the new page test is mislabelled and vacuous.**
   `snapshot/page.zig:1412` says "The largest capacity that still fits remains
   acceptable", but the header at `:1413-1422` is 80x24 with every capacity field
   zero — the *smallest* possible. The trailing assertion at `:1424-1427` re-checks
   `Page.layout(capacity).total_size <= max_page_size`, which is precisely the
   condition `pageCapacity` enforces at `:593-597` and which the preceding `try`
   already proved. It cannot fail independently. Nothing exercises a capacity just
   below the limit, so an over-strict bound would pass this test unchanged. Fix the
   comment and add a genuine near-limit accepted case.

4. **P2 — the native red/green proof is Debug-only, in the one place where the
   build mode is the whole argument.** Both `mutation-red.log` files record
   `compile test Debug aarch64-macos.13.0`, and neither directory contains the
   string `ReleaseFast`, `ReleaseSafe` or `Doptimize`. But the page-capacity
   diagnosis and `pageCapacity`'s own comment (`snapshot/page.zig:584-592`) rest
   entirely on ReleaseFast *dropping* the assertion — and
   `page-capacity/native-tests/mutation-red.log` shows the mutant aborting on
   `panic: integer does not fit in destination type` at `size.zig:126`, i.e. the
   safety check that ReleaseFast removes. The recorded red proof therefore
   exercises the opposite build mode from the one the severity claim describes.
   The sibling native evidence directory did this correctly:
   `bitmap-capacity-independent/source.json` records `"tests": {"ReleaseSafe": 33,
   "ReleaseFast": 33}`. Either add a ReleaseFast run, or state in the diagnosis
   that the out-of-bounds behavior is reasoned rather than observed under the
   corpus's own build mode.

### Worth noting

5. `ReplayStart`/`replayStart`/`c1Introducer` should be file-scope, beside
   `findVTReplayStart` and `findUtf8ReplayStart`, rather than nested in `Tracker`
   (`stream_continuation.zig:190-255`). They are pure functions of the byte slice;
   nesting them is what blocks a direct unit test in the module that owns them.

6. `stream_continuation.zig` gained **no test at all**, in a file whose eight
   existing tests (`:499`, `:520`, `:537`, `:555`, `:581`, `:599`, `:630`, `:667`)
   pin every other helper individually. Every assertion about the new logic lives
   in `stream.zig`, one abstraction level up and behind a `Stream`.

7. The rule for "which `Effect` means replayed committed work" now lives at three
   call sites with three different comparisons and no shared predicate: `validate`
   `!= .uncommitted` (`:54`), `replayStart` `== .committed` (`:219`), `write`
   `== .omittable` (`:282`), against the enum at `:405-419`. I traced all three
   and each is correct today, but only the enum links them; a fourth `Effect`
   variant would require finding all three by hand. A named predicate would
   localise the intent.

8. `continuation_buf` runs the exporter twice in two of its paths: once on the
   null/zero size-query form (`c/terminal.zig:1052-1060`) and again when a fixed
   writer overflows and it re-runs against a counter (`:1066-1074`). Each run is
   now two scans instead of one, so those paths went from two passes to four over
   `continuation_max_bytes`. Bounded and per-call, not per-byte — but it is the
   one place where the two-pass structure compounds, and it is unmeasured.

9. Doc comments the patch left stale, and a real divergence underneath them.
   `Tracker`'s contract at `:89-93` still says the retained bytes "always begin at
   the replay start" and that `write` only "leaves out those bytes"; neither is
   what `write` does now. `ValidateError.NonCanonicalContinuation` at `:20-21`
   still says only "A later ESC supersedes earlier VT parser state". The
   substantive part: `validate` still computes canonicality with the ESC-only
   `findVTReplayStart` (`:62-65`), so `validate` and `write` no longer share one
   definition of the canonical start. A hand-built continuation opening at an
   `ESC` but containing a later C1 introducer passes `validate` yet is not what
   `write` would emit. Nothing produced in-tree hits this, and
   `restoreContinuation`'s byte-identity compare (`c/terminal.zig:766`) backstops
   it — but the module now holds two definitions of "canonical" while documenting
   one. Document the divergence deliberately or remove it.

10. `docs/upstream/` status mismatch. Both drafts open "**DRAFT** …
    **Before posting, read the notes at the bottom of this file.**" and close with
    "Do not open a PR yet", while `81960a2`'s message and
    `scripts/native/patches/README.md:141-142` assert they are already filed as
    Ghostty discussions 14185 and 14186. I grepped: neither number appears anywhere
    in `docs/upstream/`. Add a status line per draft with the discussion number and
    the date filed.

11. `scripts/native/patches/README.md:141` calls both new defects "snapshot
    decoder defects". The continuation defect is an export/encode defect — the
    encode aborts at `snapshot/snapshot.zig:51` and nothing is decoded — as
    `terminal-continuation-c1-introducer.md` itself says.

12. `continuation-c1/` is internally inconsistent with its own sibling.
    `seed509-after.log` (444 bytes) and `seed509-pre-existing.log` (532 bytes) are
    bare logs at the *directory root* with no `metadata.json`, while the analogous
    `page-capacity/seed201/` is a proper run directory with a full 301-file
    `metadata.json` recording platform, exit code and revision. The only
    `source.json` that pins the seed509 logs sits in a *sibling* subdirectory
    (`native-tests/`) and reaches up to describe them in its `note`. Either give
    seed509 its own run directory or move the pinning to where the logs are.

13. `95710a9` also added `docs/verification/resumed/README.md`, a 71-line index for
    a different milestone's evidence directory, in a commit titled "Reject snapshot
    page capacities…". Defensible — `terminal-page-capacity-admission.md:22` cites
    `docs/verification/resumed/native-seed201` — but it mixes scope.

14. Naming rot around the identity, in three coupled places: the file is still
    `snapshot-pending-wrap.patch` (defect 1 of 7); `verify_source.py:2` still
    describes "the reviewed snapshot roundtrip patch" when three of seven defects
    are not roundtrip defects; and `mod.rs:13` carries the same stale word inside
    the hashed marker as `snapshot-wrap-`. Rename at the next identity rotation,
    since renaming rotates the marker anyway.

15. `PREVIOUS_PAGE_LIST_SHA256S` (`verify_source.py:34`) is a file-specific escape
    hatch, hardcoded against `pathlib.Path("src/terminal/PageList.zig")` at `:78`.
    Every supersession so far either added a file — whose intermediate state is the
    recorded original, already accepted under `--prepare` — or changed
    `PageList.zig`. One special case has sufficed because of the history, not
    because of the design. Generalize before a correction touches an
    already-corrected file other than `PageList.zig`.

16. `scripts/tests/test_native_source.py:66-74` is not as file-agnostic as it
    looks. `test_growth_only_patch_upgrades_to_wide_cutoff_patch` reaches the
    `PREVIOUS_PAGE_LIST_SHA256S` branch only because `self.targets[1]` — index 1 of
    `tuple(native.TARGET_HASHES)` — happens to be `PageList.zig`. It fails loudly
    rather than silently if the dict is reordered, so this is a robustness note,
    not a hole. It does qualify the "fully parameterised" praise below.

17. `replayStart`'s fallback (`:242-243`) returns `.{}` — the pre-patch behavior —
    whenever committed work exists but no introducer was found in the retained
    bytes, or the scan ends at ground. That is a deliberate graceful degradation
    and I could not construct a reachable case for it (a feed ending at ground
    triggers `reset`, and one ending mid-codepoint triggers `replace` at the lead
    byte, both of which drop the committed prefix). But no test pins that arm, and
    its reachability is a correctness-reviewer question, not mine. Flagging it so
    it is not assumed to have been checked.

## Done well

The verifier's staging design (`verify_source.py:86-102`) — patch pristine copies
in a temp directory, verify every result against `TARGET_HASHES`, and only then
publish, all-or-nothing — is the right shape, and it is why growing from three
patched files to six across these two commits required no new logic. I confirmed
this rather than assuming it: the `verify_source.py` diffs in both commits are
pure data (new `TARGET_HASHES` entries plus the `PATCH_SHA256` rotation) plus one
comment rewording, and **neither commit touched
`scripts/tests/test_native_source.py`** at all, because it parameterises over
`TARGET_HASHES` rather than naming files (subject to finding 16).

The `page.zig` correction is at the right layer and the right granularity: a
decode-boundary admission check on the single `Header.pageCapacity` funnel
(`:570`), reached from `:200` (`Decoder.init`), `:205` (`Decoder.capacity`) and
`:352` (`decodePayload`) — which is why one check covers both the live-screen and
history paths. It reuses `CapacityError`, already composed into
`PayloadDecodeError` at `:122-123`, so no error set is enumerated by hand and the
new variant propagates to `DecodeError` (`:154`) for free. It rejects before any
allocation, and it measures the *assembled layout* rather than each field, so
capacities that only exceed the limit in combination are caught by the same rule.

One design nit inside that otherwise-good shape: `Decoder.capacity` (`:205`) is
`return self.header.pageCapacity() catch unreachable;`. The `catch unreachable`
predates this patch, but the patch widens the invariant it depends on from "the
dimensions were checked" to "the dimensions *and* the assembled layout were
checked", and `unreachable` is UB under ReleaseFast — the exact build mode this
correction exists to protect. It also recomputes the whole layout a second time.
Storing the validated `TerminalPageCapacity` on the `Decoder` in `init` (`:200`)
and returning it from `capacity()` would delete both the `catch unreachable` and
the duplicate computation.

Both diagnosis docs state their scope limits explicitly and decline to claim
release acceptance (`terminal-page-capacity-admission.md:107-108`). The
`seed509-pre-existing.log` evidence — rebuilding without the first correction to
show the second defect predates it, and capturing the identical `EngineFailure`
panic — is more rigour than the change required.

## Corrections made to the interrupted draft

Listed because several inverted a conclusion, not merely a line number.

| Draft claim | Verified fact |
|---|---|
| "whose only production callers are `snapshot.zig` and `stream_terminal.zig`" | Backwards. Every `writeContinuation` call in those two files is test code. Production callers are `c/terminal.zig:761` and `:958`. |
| "the new scan runs only at export" (accepted as stated) | Also runs on the restore path (`restoreContinuation`, `c/terminal.zig:761`), and twice per call in two `continuation_buf` paths. Feed path genuinely untouched. |
| "neither directory carries the `README.md` that 35 other verification directories do" (finding 14) | 24 of 67 directories have one; 43 do not. Absence is the majority. Finding withdrawn. |
| "both carry a `note` … which matches `bitmap-capacity-independent/`" and "none was invented" | `bitmap-capacity-independent/source.json` has no `note` and a different schema. These are the only 2 of 36 `source.json` files with a `note`. A new convention *was* invented. |
| "`terminal-page-capacity-admission.md:83` makes the same citation" | That file never names `full-green.log`. The equivalent unevidenced claim is at `:93-94`. |
| "`terminal-continuation-c1-introducer.md:88`" | The citation is at `:96`. |
| "five superseded-identity paragraphs" / "five identity rotations" | Six paragraphs (`README.md:105-139`); six rotations, seven identities. |
| "a twelve-row table" | 13 rows: twelve file rows plus the patch row (`:59-73`). |
| "`README.md:97-103`" for the fixed-patch sentence | The sentence is `:97-99`; `:99-106` is the diagnoses list. |
| "the eleven other `stream: continuation` tests" | Ten pre-existing. |
| "first `test` at `stream.zig:3085`" | `:3079` (`test Action`). |
| "`parse_table.zig:111-118` sets only `0x00-0x7F`" | Block is `:111-120`; the range that completes `0x7F` is at `:119`. Overrides cited as `osc_string:349`/`dcs_passthrough:241` are the block headers; the actual override lines are `:358` and `:252`ff. |
| "`Parser.zig:274`, `:287`" for self-transition suppression | The guards are `:275` and `:288`; 274/287 are comments. |
| "would bury a 300-line delta" | +447/-12 across 19 hunks. |
| Patch nonblank = 598 in the identity table | 613 (the file has no blank lines). |
| "each defect has its own `docs/reviews/terminal-*.md` diagnosis" | Six of seven. The allocator defect is documented in `bitmap-capacity-*.md`, which `README.md:99-106` omits. |
| "reachable only from `README.md:142` and the two diagnoses" (finding 12) | Only `README.md:142` names the directory; the diagnoses cite discussion numbers only. And `docs/README.md` indexes no review or verification directory, so this is not a deviation — finding withdrawn. |
| "`docs/upstream/` … two directories deep with two files" | One directory, two files. |
| metadata keys "argv … toolchain" | The keys are `command` and `rustc`. |

Findings 4, 8, 12, 16 and 17, and the `Decoder.capacity` nit, are new here and
were not in the draft.

## Reviewed identities

Every digest below was recomputed during this review and matches
`verify_source.py` and the README table.

| File | Nonblank lines | SHA-256 |
|---|---:|---|
| `scripts/native/patches/snapshot-pending-wrap.patch` | 613 | `745b5e98703259704c7ad9a5c2e1357817efda85cb6f60cd3db730d6d42f4947` |
| `scripts/native/patches/README.md` | 125 | `e6b21609d8e5682eb95c61e406350759e683ae580e59e3991d983586c7ddb6ab` |
| `scripts/native/verify_source.py` | 114 | `ee1f6b1ffeaf028b82d8b75380c422aada2c2fd6586096354cfc36c5fb217dfa` |
| `docs/reviews/terminal-page-capacity-admission.md` | 88 | `ce71f347a99d7f3895873369d74aec30b17047dccab56b94e8493208868c1a88` |
| `docs/reviews/terminal-continuation-c1-introducer.md` | 94 | `4d0db1c940e50ed55367717c11d9845bace30e958b763297d827eedbd7bb976b` |
| `docs/upstream/ghostty-issue-triage-page-capacity.md` | 122 | `704ace419006b8e6a70bfe55545faac53cf140d6d514330cac8c2a43a29355fb` |
| `docs/upstream/ghostty-issue-triage-continuation-c1.md` | 112 | `58d826658f29821bc8079a302fda9af329d8293498001e143e4634351636eeec` |

Corrected vendored sources reviewed at the identities recorded in
`verify_source.py:14-33`, each recomputed and matching: `stream_continuation.zig`
`8d36a4991ce7a9432857e2d12052d5f212c0faac619ff3e38901b11ada726734`, `stream.zig`
`7e2f63d504bdf558de243b837017a834a951faeecbe7691218a217b11d75f6bc`,
`snapshot/page.zig`
`3d10248e58c4b4019ff463e6bcc829fe808e82b9c88a448433d4cc7a178f5052`. I did not
rebuild the library, so `library_sha256` in the build stamp remains unverified by
this review.
